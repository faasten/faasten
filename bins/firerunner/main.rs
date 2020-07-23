#[macro_use(crate_version, crate_authors)]
extern crate clap;
extern crate cgroups;

use std::fs::File;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::io::BufReader;


use vmm::vmm_config::boot_source::BootSourceConfig;
use vmm::vmm_config::drive::BlockDeviceConfig;
use vmm::vmm_config::net::NetworkInterfaceConfig;
use net_util::MacAddr;
use vmm::vmm_config::vsock::VsockDeviceConfig;
use vmm::vmm_config::machine_config::VmConfig;
use vmm::SnapFaaSConfig;
//use vmm::vmm_config::logger::{LoggerConfig, LoggerLevel};

use clap::{App, Arg};

use snapfaas::vm;
use snapfaas::firecracker_wrapper::VmmWrapper;

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
            Arg::with_name("kernel_args")
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
        .arg(
            Arg::with_name("copy_base_memory")
                 .long("copy_base")
                 .value_name("COPYBASE")
                 .takes_value(false)
                 .required(false)
                 .help("Restore base snapshot memory by copying")
        )
        .arg(
            Arg::with_name("hugepage")
                 .long("hugepage")
                 .value_name("HUGEPAGE")
                 .takes_value(false)
                 .required(false)
                 .help("Use huge pages to back virtual machine memory")
        )
        .arg(
            Arg::with_name("diff_dirs")
                 .long("diff_dirs")
                 .value_name("DIFFDIRS")
                 .takes_value(true)
                 .required(false)
                 .help("Comma-separated list of diff snapshots")
        )
        .arg(
            Arg::with_name("copy_diff_memory")
                 .long("copy_diff")
                 .value_name("COPYDIFF")
                 .takes_value(false)
                 .required(false)
                 .help("If a diff snapshot is provided, restore its memory by copying")
        )
        .arg(
            Arg::with_name("mac")
                 .long("mac")
                 .value_name("MAC")
                 .takes_value(true)
                 .required(false)
                 .help("configure a network device for the VM with the provided MAC address")
        )
        .arg(
            Arg::with_name("tap_name")
                 .long("tap_name")
                 .value_name("TAPNAME")
                 .takes_value(true)
                 .required(false)
                 .help("configure a network device for the VM backed by the provided tap device")
        )
        .arg(
            Arg::with_name("vsock cid")
                .long("cid")
                .value_name("CID")
                .takes_value(true)
                .required(false)
                .help("vsock cid of the guest VM")
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
                .value_of("kernel_args")
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
    let diff_dirs = cmd_arguments.value_of("diff_dirs").map_or(Vec::new(), |x| x.split(',').collect::<Vec<&str>>()
        .iter().map(PathBuf::from).collect());
    let huge_page = cmd_arguments.is_present("hugepage");
    let copy_base = cmd_arguments.is_present("copy_base_memory");
    let copy_diff = cmd_arguments.is_present("copy_diff_memory");
    let mac = cmd_arguments.value_of("mac").map(|x| x.to_string());
    let tap_name = cmd_arguments.value_of("tap_name").map(|x| x.to_string());
    assert!(tap_name.is_none() == mac.is_none());
    let cid = cmd_arguments.value_of("vsock cid")
        .map(|x| x.parse::<u32>().expect("Invalid cid"));

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

    if dump_dir.is_some() && !dump_dir.as_ref().unwrap().exists(){
        io::stdout().write_all(&[vm::VmStatus::DumpDirNotExist as u8]).expect("stdout");
        io::stdout().flush().expect("stdout");
        std::process::exit(1);
    }

    let json_dir = if let Some(dir) = diff_dirs.last() {
        Some(dir.clone())
    } else if let Some(ref dir) = load_dir {
        Some(dir.clone())
    } else {
        None
    };
    let parsed_json = json_dir.map(|mut dir| {
        dir.push("snapshot.json");
        let reader = BufReader::new(File::open(dir).expect("Failed to open snapshot.json"));
        serde_json::from_reader(reader).expect("Bad snapshot.json")
    });

    let from_snapshot = load_dir.is_some();
    let config = SnapFaaSConfig {
        parsed_json,
        memory_to_load: None,
        load_dir,
        dump_dir,
        huge_page,
        copy_base,
        copy_diff,
        diff_dirs,
    };
    // Create vmm thread
    let mut vmm = match VmmWrapper::new(instance_id, config) {
        Ok(vmm) => vmm,
        Err(e) => {
            io::stdout().write_all(&[vm::VmStatus::VmmFailedToStart as u8]).expect("stdout");
            io::stdout().flush().expect("stdout");
            eprintln!("Vmm failed to start due to: {:?}", e);
            std::process::exit(1);
        }
    };

    // Configure vm through vmm thread
    // If any of the configuration actions fail, just exits the process with
    // exit code 1. When exit happens before sending the Ready Signal, whoever
    // is listening on the stdout of this process for a ready signal will
    // receive 0.
    let machine_config = VmConfig{
	vcpu_count: Some(vcpu_count as u8),
	mem_size_mib: Some(mem_size_mib),
	..Default::default()
    };

    if let Err(e) = vmm.set_configuration(machine_config) {
        eprintln!("Vmm failed to set configuration due to: {:?}", e);
        std::process::exit(1);
    }

    if !from_snapshot {
        let boot_config = BootSourceConfig {
            kernel_image_path: kernel.to_str().expect("kernel path None").to_string(),
            boot_args: Some(kargs),
        };

        if let Err(e) = vmm.set_boot_source(boot_config) {
            eprintln!("Vmm failed to set boot source due to: {:?}", e);
            std::process::exit(1);
        }
    }

    let block_config = BlockDeviceConfig {
	drive_id: String::from("rootfs"),
	path_on_host: rootfs,
	is_root_device: true,
	is_read_only: true,
	partuuid: None,
	rate_limiter: None,
    };

    if let Err(e) = vmm.insert_block_device(block_config) {
        eprintln!("Vmm failed to insert rootfs due to: {:?}", e);
        std::process::exit(1);
    }

    if let Some(appfs) = appfs {
	let block_config = BlockDeviceConfig {
	    drive_id: String::from("appfs"),
	    path_on_host: appfs,
	    is_root_device: false,
	    is_read_only: true,
	    partuuid: None,
	    rate_limiter: None,
	};
        if let Err(e) = vmm.insert_block_device(block_config) {
            eprintln!("Vmm failed to insert appfs due to: {:?}", e);
            std::process::exit(1);
        }
    }

    if let Some(mac_addr) = mac {
        let netif_config = NetworkInterfaceConfig {
            iface_id: String::from("eth0"),
            host_dev_name: tap_name.unwrap(),
            guest_mac: Some(MacAddr::parse_str(mac_addr.as_str()).expect("MacAddr")),
            rx_rate_limiter: None,
            tx_rate_limiter: None,
            allow_mmds_requests: false,
            tap: None,
        };
        if let Err(e) = vmm.insert_network_device(netif_config) {
            eprintln!("Vmm failed to insert network device due to: {:?}", e);
            std::process::exit(1);
        }
    }

    if let Some(cid) = cid {
        let vsock_config = VsockDeviceConfig {
            id: "vsock0".to_string(),
            guest_cid:cid,
        };
        if let Err(e) = vmm.add_vsock(vsock_config) {
            eprintln!("Vmm failed to add vsock due to: {:?}", e);
            std::process::exit(1);
        }
    }
 
    //TODO: Optionally add a logger

    // Launch vm
    if let Err(e) = vmm.start_instance() {
        eprintln!("Vmm failed to start instance due to: {:?}", e);
        std::process::exit(1);
    }

    //// wait for ready notification from vm
    //let ret = match vm.recv_status() {
    //    Ok(d) => d,
    //    Err(e) => {
    //        eprintln!("Failed to receive ready signal due to: {:?}", e);
    //        io::stdout().write_all(&[vm::VmStatus::NoReadySignal as u8]).expect("stdout");
    //        io::stdout().flush().expect("stdout");
    //        std::process::exit(1);
    //    }
    //};

    //// notify snapfaas that the vm is ready
    //io::stdout().write_all(READY).expect("stdout");
    //io::stdout().flush().expect("stdout");
    //eprintln!("VM with notifier id {} is ready", u32::from_le_bytes(ret));

    //let mut req_count = 0;

    //loop {
    //    // Read a request from stdin
    //    // First read the 8 bytes ([u8;8]) that encodes the number of bytes in
    //    // payload as a big-endian number
    //    let mut req_header = [0;8];
    //    let stdin = io::stdin();
    //    // XXX: This read blocks when there's nothing in stdin
    //    stdin.lock().read_exact(&mut req_header).expect("stdin");
    //    let size = u64::from_be_bytes(req_header);

    //    let mut req_buf = vec![0; size as usize];
    //    stdin.lock().read_exact(&mut req_buf).expect("stdin");

    //    let mut req = String::from_utf8(req_buf).expect("not json string");
    //    req.push('\n');

    //    // Send request to vm as [u8]
    //    let rsp = match vm.send_request_u8(req.as_bytes()) {
    //        Ok(()) => {
    //            // Wait and receive response from vm
    //            match vm.recv_response_string() {
    //                Ok(s) => s,
    //                Err(e) => {
    //                    eprintln!("Failed to receive response from vm due to: {:?}", e);
    //                    VM_RESPONSE_ERROR.to_string()
    //                }
    //            }
    //        }
    //        Err(e) => {
    //            eprintln!("Failed to send request to vm due to: {:?}", e);
    //            VM_REQUEST_ERROR.to_string()
    //        }
    //    };

    //    // first write the number bytes in the response to stdout
    //    let rsp_buf = rsp.as_bytes();
    //    let size = rsp_buf.len().to_be_bytes();

    //    let mut data = rsp_buf.to_vec();
    //    for i in (0..size.len()).rev() {
    //        data.insert(0, size[i]);
    //    }
    //    io::stdout().write_all(&data).expect("stdout");
    //    io::stdout().flush().expect("stdout");

    //    req_count = req_count+1;
    //}

    //vmm.shutdown_instance();
    vmm.join_vmm();
    std::process::exit(0);
}
