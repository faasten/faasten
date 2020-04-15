#[macro_use(crate_version, crate_authors)]
extern crate clap;
extern crate cgroups;

use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::os::unix::io::FromRawFd;
use std::path::{Path, PathBuf};

use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

use futures::Future;
use futures::sync::oneshot;
use vmm::{VmmAction, VmmActionError, VmmData};
use vmm::vmm_config::instance_info::{InstanceInfo, InstanceState};
use vmm::vmm_config::boot_source::BootSourceConfig;
use vmm::vmm_config::drive::BlockDeviceConfig;
use vmm::vmm_config::vsock::VsockDeviceConfig;
use vmm::vmm_config::machine_config::VmConfig;
use vmm::vmm_config::logger::{LoggerConfig, LoggerLevel};
use sys_util::EventFd;

use clap::{App, Arg};
use serde_json::Value;
use time::precise_time_ns;

use snapfaas::vm;
use snapfaas::firecracker_wrapper;

const READY :&[u8] = &[vm::VmStatus::Ready as u8];


fn main() {
    let cmd_arguments = App::new("firecracker")
        .version(crate_version!())
        .author(crate_authors!())
        .about("launch a microvm.")
        .arg(
            Arg::with_name("kernel")
                .short("k")
                .long("kernel")
                .value_name("kernel")
                .takes_value(true)
                .required(true)
                .help("path the the kernel binary")
        )
        .arg(
            Arg::with_name("kernel boot args")
                .short("c")
                .long("kernel_args")
                .value_name("kernel_args")
                .takes_value(true)
                .required(false)
                .default_value("quiet console=none reboot=k panic=1 pci=off")
                .help("kernel boot args")
        )
        .arg(
            Arg::with_name("rootfs")
                .short("r")
                .long("rootfs")
                .value_name("rootfs")
                .takes_value(true)
                .required(true)
                .help("path to the root file system")
        )
        .arg(
            Arg::with_name("appfs")
                .long("appfs")
                .value_name("appfs")
                .takes_value(true)
                .required(false)
                .help("path to the root file system")
        )
        .arg(
            Arg::with_name("id")
                .long("id")
                .help("microvm unique identifier")
                .default_value("abcde1234")
                .required(true)
                .takes_value(true)
        )
        .arg(
            Arg::with_name("load_dir")
                .long("load_from")
                .takes_value(true)
                .required(false)
                .help("if specified start vm from a snapshot under the given directory")
        )
        .arg(
            Arg::with_name("dump_dir")
                .long("dump_to")
                .takes_value(true)
                .required(false)
                .help("if specified creates a snapshot right after runtime is up under the given directory")
        )
        .arg(
            Arg::with_name("mem_size")
                 .long("mem_size")
                 .value_name("MEMSIZE")
                 .takes_value(true)
                 .required(true)
                 .help("Guest memory size in MB (default is 128)")
        )
        .arg(
            Arg::with_name("vcpu_count")
                 .long("vcpu_count")
                 .value_name("VCPUCOUNT")
                 .takes_value(true)
                 .required(true)
                 .help("Number of vcpus (default is 1)")
        )
        .get_matches();

    // process command line arguments
    let instance_id = cmd_arguments.value_of("id").unwrap().to_string();
    let kernel = cmd_arguments
                .value_of("kernel")
                .map(PathBuf::from)
                .expect("path to kernel image not specified");
    let rootfs = cmd_arguments
                .value_of("rootfs")
                .map(PathBuf::from)
                .expect("path to rootfs not specified");

    let kargs = cmd_arguments
                .value_of("kernel boot args")
                .expect("kernel boot argument not specified")
                .to_string();
    let mem_size_mib = cmd_arguments
                .value_of("mem_size")
                .map(|x| x.parse::<usize>().expect("Invalid VM mem size"))
                .expect("VM mem size not specified");
    let vcpu_count = cmd_arguments
                .value_of("vcpu_count")
                .map(|x| x.parse::<u64>().expect("Invalid vcpu count"))
                .expect("vcpu count not specified");

    // optional arguments:
    let appfs = cmd_arguments.value_of("appfs").map(PathBuf::from);
    let load_dir: Option<PathBuf> = cmd_arguments.value_of("load_dir").map(PathBuf::from);
    let dump_dir: Option<PathBuf> = cmd_arguments.value_of("dump_dir").map(PathBuf::from);

    // Make sure kernel, rootfs, appfs, load_dir, dump_dir exist
    if !&kernel.exists() {
        io::stdout().write_all(&[vm::VmStatus::KernelNotExist as u8]).expect("stdout");
        io::stdout().flush().expect("stdout");
        std::process::exit(1);
    }
    if !&rootfs.exists() {
        io::stdout().write_all(&[vm::VmStatus::RootfsNotExist as u8]).expect("stdout");
        io::stdout().flush().expect("stdout");
        std::process::exit(1);
    }

    if appfs.is_some() && !appfs.as_ref().unwrap().exists(){
        io::stdout().write_all(&[vm::VmStatus::AppfsNotExist as u8]).expect("stdout");
        io::stdout().flush().expect("stdout");
        std::process::exit(1);
    }

    if load_dir.is_some() && !load_dir.as_ref().unwrap().exists(){
        io::stdout().write_all(&[vm::VmStatus::LoadDirNotExist as u8]).expect("stdout");
        io::stdout().flush().expect("stdout");
        std::process::exit(1);
    }


    // output file for debugging
    /*
    let file_name = format!("out/vm-{}.log", instance_id);
    let mut output_file = File::create(file_name)
                          .expect("Cannot create output file. Make sure out/ is created in the current directory.");
    write!(&mut output_file, "id: {:?}\n", instance_id);
    write!(&mut output_file, "memory size: {:?}\n", mem_size_mib);
    write!(&mut output_file, "vcpu count: {:?}\n", vcpu_count);
    write!(&mut output_file, "rootfs: {:?}\n", rootfs);
    write!(&mut output_file, "appfs: {:?}\n", appfs);
    write!(&mut output_file, "load dir: {:?}\n", load_dir);
    write!(&mut output_file, "dump_dir: {:?}\n", dump_dir);
    //output_file.flush();
    */

    // Create vmm thread
    let (request_receiver, request_sender) = nix::unistd::pipe().expect("Failed to create request pipe");
    let (response_receiver, response_sender) = nix::unistd::pipe().expect("Failed to create response pipe");
    let (ready_receiver, ready_sender) = nix::unistd::pipe().expect("Could not create ready notifier pipe");
    // mpsc channel for Box<VmmAction> with the vmm thread
    let (vmm_action_sender, vmm_action_receiver) = channel();
    let shared_info = Arc::new(RwLock::new(InstanceInfo {
			state: InstanceState::Uninitialized,
			id: instance_id.clone(),
			vmm_version: "0.1".to_string(),
			load_dir: load_dir,
			dump_dir: dump_dir,
			}));

    let event_fd = Rc::new(EventFd::new().expect("Cannot create EventFd"));

    let thread_handle =
	vmm::start_vmm_thread(shared_info.clone(),
			      event_fd.try_clone().expect("Couldn't clone event_fd"),
                              vmm_action_receiver,
                              0, //seccomp::SECCOMP_LEVEL_NONE,
	                      Some(unsafe { File::from_raw_fd(response_sender) }),
	                      Some(unsafe { File::from_raw_fd(request_receiver) }),
                              Some(unsafe { File::from_raw_fd(ready_sender) }),
                              instance_id.parse::<u32>().unwrap() //TODO: remove notifier ID completely. Just pass in a dummy value for now
                              );


    // Configure vm through vmm thread
    let machine_config = VmConfig{
	vcpu_count: Some(vcpu_count as u8),
	mem_size_mib: Some(mem_size_mib),
	..Default::default()
    };

    let (sync_sender, sync_receiver) = oneshot::channel();
    let action = VmmAction::SetVmConfiguration(machine_config, sync_sender);
    vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Failed to send SetVmConfiguration");
    event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
    let action_ret = sync_receiver.wait().expect("set config");
    //println!("Set vm configuration: {:?}", action_ret);
//    write!(&mut output_file, "Set vm configuration: {:?}\n", action_ret);

    let boot_config = BootSourceConfig {
	kernel_image_path: kernel.to_str().expect("kernel path None").to_string(),
	boot_args: Some(kargs),
    };

    let (sync_sender, sync_receiver) = oneshot::channel();
    let action = VmmAction::ConfigureBootSource(boot_config, sync_sender);
    vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Failed to send SetVmConfiguration");
    event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
    let action_ret = sync_receiver.wait().expect("set config");
    //println!("Set boot source: {:?}", action_ret);


    let block_config = BlockDeviceConfig {
	drive_id: String::from("rootfs"),
	path_on_host: rootfs,
	is_root_device: true,
	is_read_only: true,
	partuuid: None,
	rate_limiter: None,
    };

    let (sync_sender, sync_receiver) = oneshot::channel();
    let action = VmmAction::InsertBlockDevice(block_config, sync_sender);
    vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Failed to send SetVmConfiguration");
    event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
    let action_ret = sync_receiver.wait().expect("rootfs");
    //println!("Insert rootfs: {:?}", action_ret);

    if let Some(appfs) = appfs {
	let block_config = BlockDeviceConfig {
	    drive_id: String::from("appfs"),
	    path_on_host: appfs,
	    is_root_device: false,
	    is_read_only: true,
	    partuuid: None,
	    rate_limiter: None,
	};
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::InsertBlockDevice(block_config, sync_sender);
        vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Failed to send SetVmConfiguration");
        event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
        let action_ret = sync_receiver.wait().expect("rootfs");
        //println!("Insert appfs: {:?}", action_ret);
    }

 
    let (sync_sender, sync_receiver) = oneshot::channel();
    let action = VmmAction::GetVmConfiguration(sync_sender);
    vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Couldn't send");
    event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
    let action_ret = sync_receiver.wait().expect("set config");
    //println!("Get vm configuration: {:?}", action_ret);
   // write!(&mut output_file, "Get vm configuration: {:?}\n", action_ret);

    /*
    let logger_config = LoggerConfig {
        log_fifo: "vm-boot-log.log".to_string(),
        metrics_fifo: "vm-metrics.log".to_string(),
        level: LoggerLevel::Debug,
        show_level: true,
        show_log_origin: true,
        options: Value::String("LogDirtyPages".to_string()),
    };
    let (sync_sender, sync_receiver) = oneshot::channel();
    let action = VmmAction::ConfigureLogger(logger_config, sync_sender);
    vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Couldn't send");
    event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
    let action_ret = sync_receiver.wait().expect("set config");
    println!("logger config: {:?}", action_ret);
    */

    // launch vm
//    write!(&mut output_file, "Launching VM\n");

    //let t1 = precise_time_ns();
    let (sync_sender, sync_receiver) = oneshot::channel();
    let action = VmmAction::StartMicroVm(sync_sender);
    vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Couldn't send");
    event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
    let action_ret = sync_receiver.wait().expect("set config");
    //println!("Start vm: {:?}", action_ret);
//    write!(&mut output_file, "Start VM: {:?}\n", action_ret);


    // wait for ready notification from vm
    let data = &mut[0u8; 4usize];
    unsafe{ File::from_raw_fd(ready_receiver) }.read_exact(data).expect("Failed to receive ready signal");
//    write!(&mut output_file, "Started VM: {:?}\n", action_ret);
//    write!(&mut output_file, "VM with notifier id {:?} is ready\n", data);
//    output_file.flush();
    //println!("VM with notifier id {} is ready", u32::from_le_bytes(*data));

    //let t2 = precise_time_ns();
    //write!(&mut output_file, "boot_time: {:?}\n", t2-t1);
//    output_file.flush();

    // notify snapfaas that the vm is ready
    io::stdout().write_all(READY);
    io::stdout().flush();
    //println!("VM with notifier id {} is ready", u32::from_le_bytes(*data));

    // request and response process loop
    let mut request_sender = unsafe { File::from_raw_fd(request_sender) };
    let mut response_receiver = unsafe { File::from_raw_fd(response_receiver) };
    let mut req_count = 0;
    // continuously read from stdio
    loop {
        let mut req_header = [0;8];
        let mut stdin = io::stdin();
        // does this block when there's nothing in stdin?
        // first read in the number of bytes that I should read from stdin
        stdin.lock().read_exact(&mut req_header);
        let size = u64::from_be_bytes(req_header);

        // actually read the request
        let mut req_buf = vec![0; size as usize];
        stdin.lock().read_exact(&mut req_buf);

        let mut req = String::from_utf8(req_buf).expect("not json string");
//        write!(&mut output_file, "request: {:?}\n",req);
        req.push('\n');

        request_sender.write_all(req.as_bytes());

        let mut lens = [0; 4];
        response_receiver.read_exact(&mut lens).expect("Failed to read response size");
        let len = u32::from_be_bytes(lens);
        let mut response = vec![0; len as usize];
        response_receiver.read_exact(response.as_mut_slice()).expect("Failed to read response");
        let rsp =  String::from_utf8(response).unwrap();


        //let rsp= lipsum_words(12);

        //stdout.lock().write_fmt(format_args!("success"));
        // first write the number bytes in the response to stdout

        let req_buf = rsp.as_bytes();
        let mut size = req_buf.len().to_be_bytes();
        let mut data = req_buf.to_vec();
        for i in (0..size.len()).rev() {
            data.insert(0, size[i]);
        }
        io::stdout().write_all(&data);
        io::stdout().flush();

        req_count = req_count+1;
        //write!(&mut output_file, "Done: {}\n", req_count);
        //output_file.flush();
        //stdout.lock().write_fmt(format_args!("echo: {:?}", req_buf));
    }

    // shutdown vm
    let (sync_sender, sync_receiver) = oneshot::channel();
    let action = VmmAction::SendCtrlAltDel(sync_sender);
    vmm_action_sender.send(Box::new(action)).map_err(|_| ()).expect("Couldn't send");
    event_fd.write(1).map_err(|_| ()).expect("Failed to signal");
    let action_ret = sync_receiver.wait().expect("set config");
    println!("Shutdown vm: {:?}", action_ret);


    std::process::exit(0);

    /*
    
    let mut req_count = 0;
    // continuously read from stdio
    loop {
        let mut req_header = [0;8];
        let mut stdin = io::stdin();
        // does this block when there's nothing in stdin?
        // first read in the number of bytes that I should read from stdin
        stdin.lock().read_exact(&mut req_header);
        let size = u64::from_be_bytes(req_header);

        // actually read the request
        let mut req_buf = vec![0; size as usize];
        stdin.lock().read_exact(&mut req_buf);

        // sleep to simulate running
        std::thread::sleep(std::time::Duration::from_millis(wait_time as u64));
        write!(&mut output_file, "process time: {}\n", wait_time);

        let rsp= match appfs.as_ref() {
            "lorempy2.ext4" => lipsum_words(12),
            "loremjs.ext4" => lipsum(12),
            "sentiment-analysis.ext4" => "{\"subjectivity: 0.8\", \"polarity\":0.2}".to_string(),
            _ => "done".to_string(),
        };



        //stdout.lock().write_fmt(format_args!("success"));
        // first write the number bytes in the response to stdout

        let req_buf = rsp.as_bytes();
        let mut size = req_buf.len().to_be_bytes();
        let mut data = req_buf.to_vec();
        for i in (0..size.len()).rev() {
            data.insert(0, size[i]);
        }
        io::stdout().write_all(&data);
        io::stdout().flush();

        req_count = req_count+1;
        write!(&mut output_file, "Done: {}\n", req_count);
        output_file.flush();
        //stdout.lock().write_fmt(format_args!("echo: {:?}", req_buf));
    }
    */

    /*
    let (checker, notifier) = nix::unistd::pipe().expect("Could not create a pipe");

    let mut app = VmAppConfig {
        kernel,
        instance_id,
        rootfs,
        appfs,
        cmd_line,
        seccomp_level,
        vsock_cid: 42,
        notifier: unsafe{ File::from_raw_fd(notifier) },
        cpu_share: 1024,
        vcpu_count: vcpu_count.unwrap_or(1),
        mem_size_mib,
        load_dir,
        dump_dir,
    }.run(true, None);

    // We need to wait for the ready signal from Firecracker
    let data = &mut[0u8; 4usize];
    unsafe{ File::from_raw_fd(checker) }.read_exact(data).expect("Failed to receive ready signal");
    println!("VM with notifier id {} is ready", u32::from_le_bytes(*data));

    let stdin = std::io::stdin();

    for mut line in stdin.lock().lines().map(|l| l.unwrap()) {
        line.push('\n');
        app.connection.write_all(line.as_bytes()).expect("Failed to write to request pipe");
        let mut lens = [0; 4];
        app.connection.read_exact(&mut lens).expect("Failed to read response size");
        let len = u32::from_be_bytes(lens);
        let mut response = vec![0; len as usize];
        app.connection.read_exact(response.as_mut_slice()).expect("Failed to read response");
        println!("{}", String::from_utf8(response).unwrap());
    }
    app.kill();
    */
}
