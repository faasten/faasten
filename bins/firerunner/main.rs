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
use snapfaas::firecracker_wrapper::{self, VmmWrapper, VmChannel};

const READY :&[u8] = &[vm::VmStatus::Ready as u8];
const VM_RESPONSE_ERROR: &str = "VmResposneError";
const VM_REQUEST_ERROR: &str = "VmRequestError";


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
    let instance_id = cmd_arguments.value_of("id").expect("id doesn't exist").to_string();
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
    let dump = dump_dir.is_some();

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
    let (mut vmm, mut vm) = match VmmWrapper::new(instance_id, load_dir, dump_dir) {
        Ok((vmm, vm)) => (vmm, vm),
        Err(e) => {
            io::stdout().write_all(&[vm::VmStatus::VmmFailedToStart as u8]).expect("stdout");
            io::stdout().flush().expect("stdout");
            eprintln!("Vmm failed to start due to: {:?}", e);
            std::process::exit(1);
        }
    };

    // Configure vm through vmm thread
    let machine_config = VmConfig{
	vcpu_count: Some(vcpu_count as u8),
	mem_size_mib: Some(mem_size_mib),
	..Default::default()
    };

    let ret = match vmm.set_configuration(machine_config) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Vmm failed to set configuration due to: {:?}", e);
            std::process::exit(1);
        }
    };
    // write!(&mut output_file, "Set vm configuration: {:?}\n", action_ret);

    let boot_config = BootSourceConfig {
	kernel_image_path: kernel.to_str().expect("kernel path None").to_string(),
	boot_args: Some(kargs),
    };

    let ret = match vmm.set_boot_source(boot_config) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Vmm failed to set boot source due to: {:?}", e);
            std::process::exit(1);
        }
    };

    let block_config = BlockDeviceConfig {
	drive_id: String::from("rootfs"),
	path_on_host: rootfs,
	is_root_device: true,
	is_read_only: true,
	partuuid: None,
	rate_limiter: None,
    };

    let ret = match vmm.insert_block_device(block_config) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Vmm failed to insert rootfs due to: {:?}", e);
            std::process::exit(1);
        }
    };

    if let Some(appfs) = appfs {
	let block_config = BlockDeviceConfig {
	    drive_id: String::from("appfs"),
	    path_on_host: appfs,
	    is_root_device: false,
	    is_read_only: true,
	    partuuid: None,
	    rate_limiter: None,
	};

        let ret = match vmm.insert_block_device(block_config) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Vmm failed to insert appfs due to: {:?}", e);
                std::process::exit(1);
            }
        };
    }

 
    let ret = match vmm.get_configuration() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Vmm failed to get configuration due to: {:?}", e);
            std::process::exit(1);
        }
    };

    //TODO: Optionally add a logger


    // Launch vm
    let ret = match vmm.start_instance() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Vmm failed to get configuration due to: {:?}", e);
            std::process::exit(1);
        }
    };



    // wait for ready notification from vm
    let ret = match vm.recv_status() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to receive ready signal due to: {:?}", e);
            io::stdout().write_all(&[vm::VmStatus::NoReadySignal as u8]).expect("stdout");
            io::stdout().flush().expect("stdout");
            std::process::exit(1);
        }
    };

    // notify snapfaas that the vm is ready
    io::stdout().write_all(READY).expect("stdout");
    io::stdout().flush().expect("stdout");
    //eprintln!("VM with notifier id {} is ready", u32::from_le_bytes(ret));

    // request and response process loop
    let mut req_count = 0;

    loop {
        // Read a request from stdin
        // First read the 8 bytes ([u8;8]) that encodes the number of bytes in
        // payload as a big-endian number
        let mut req_header = [0;8];
        let stdin = io::stdin();
        // TODO: Make sure this read blocks when there's nothing in stdin
        stdin.lock().read_exact(&mut req_header).expect("stdin");
        let size = u64::from_be_bytes(req_header);

        let mut req_buf = vec![0; size as usize];
        stdin.lock().read_exact(&mut req_buf).expect("stdin");

        let mut req = String::from_utf8(req_buf).expect("not json string");
        req.push('\n');

        // Send request to vm as [u8]
        let rsp = match vm.send_request_u8(req.as_bytes()) {
            Ok(()) => {
                // Wait and receive response from vm
                match vm.recv_response_string() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to receive response from vm due to: {:?}", e);
                        VM_RESPONSE_ERROR.to_string()
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to send request to vm due to: {:?}", e);
                VM_REQUEST_ERROR.to_string()
            }
        };

        // first write the number bytes in the response to stdout
        let rsp_buf = rsp.as_bytes();
        let size = rsp_buf.len().to_be_bytes();

        let mut data = rsp_buf.to_vec();
        for i in (0..size.len()).rev() {
            data.insert(0, size[i]);
        }
        io::stdout().write_all(&data).expect("stdout");
        io::stdout().flush().expect("stdout");

        req_count = req_count+1;
    }

    // TODO: shutdown vm
    std::process::exit(0);
}
