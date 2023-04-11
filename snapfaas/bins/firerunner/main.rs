#[macro_use(crate_version, crate_authors)]
extern crate clap;
extern crate cgroups;

use std::fs::File;
use std::path::PathBuf;
use std::io::{BufReader};
use std::time::Instant;
use std::os::unix::net::{UnixListener, UnixStream};

use vmm::vmm_config::boot_source::BootSourceConfig;
use vmm::vmm_config::drive::BlockDeviceConfig;
use vmm::vmm_config::net::NetworkInterfaceConfig;
use net_util::MacAddr;
use vmm::vmm_config::vsock::VsockDeviceConfig;
use vmm::vmm_config::machine_config::VmConfig;
use vmm::SnapFaaSConfig;
use memory_model::MemoryFileOption;

use clap::{App, Arg};

use snapfaas::firecracker_wrapper::VmmWrapper;

fn main() {
    let mut ts_vec = Vec::with_capacity(10);
    ts_vec.push(Instant::now());
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
                .default_value(vmm::DEFAULT_KERNEL_CMDLINE)
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
        .arg(
            // by default base snapshot is not opened with O_DIRECT
            Arg::with_name("odirect base")
                .long("odirect_base")
                .value_name("ODIRECT_BASE")
                .takes_value(false)
                .required(false)
                .help("If present, open base snapshot's memory file with O_DIRECT")
        )
        .arg(
            // by default diff snapshot is opened with O_DIRECT
            Arg::with_name("no odirect diff")
                .long("no_odirect_diff")
                .value_name("NO_ODIRECT_DIFF")
                .takes_value(false)
                .required(false)
                .help("If present, open diff snapshot's memory file without O_DIRECT")
        )
        .arg(
            Arg::with_name("no odirect rootfs")
                .long("no_odirect_root")
                .value_name("NO_ODIRECT_ROOT")
                .takes_value(false)
                .required(false)
                .help("If present, open rootfs file without O_DIRECT")
        )
        .arg(
            Arg::with_name("no odirect appfs")
                .long("no_odirect_app")
                .value_name("NO_ODIRECT_APP")
                .takes_value(false)
                .required(false)
                .help("If present, open appfs file without O_DIRECT")
        )
        .arg(
            Arg::with_name("dump working set")
                .long("dump_ws")
                .value_name("DUMP_WS")
                .takes_value(false)
                .required(false)
                .help("If present, VMM will send `dump working set` action to the VM when the host signals through the unix socket connection")
        )
        .arg(
            Arg::with_name("load working set")
                .long("load_ws")
                .value_name("LOAD_WS")
                .takes_value(false)
                .required(false)
                .help("If present, VMM will load the regions contained in diff_dirs[0]/WS only effective when there is one diff snapshot.")
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
    let dump_dir: Option<PathBuf> = cmd_arguments.value_of("dump_dir").map(PathBuf::from);
    let load_dir = cmd_arguments.value_of("load_dir").map_or(Vec::new(), |x| x.split(',').collect::<Vec<&str>>()
        .iter().map(PathBuf::from).collect());
    let copy_base = cmd_arguments.is_present("copy_base_memory");
    let copy_diff = cmd_arguments.is_present("copy_diff_memory");
    let odirect_base = cmd_arguments.is_present("odirect base");
    let odirect_diff = !cmd_arguments.is_present("no odirect diff");
    let odirect_rootfs = !cmd_arguments.is_present("no odirect rootfs");
    let odirect_appfs = !cmd_arguments.is_present("no odirect appfs");
    let load_ws = cmd_arguments.is_present("load working set");
    let mac = cmd_arguments.value_of("mac").map(|x| x.to_string());
    let tap_name = cmd_arguments.value_of("tap_name").map(|x| x.to_string());
    assert!(tap_name.is_none() == mac.is_none());
    let cid = cmd_arguments.value_of("vsock cid")
        .map(|x| x.parse::<u32>().expect("Invalid cid"));

    // Make sure kernel, rootfs, appfs, load_dir, dump_dir exist
    if !&kernel.exists() {
        eprintln!("kernel not exist");
        std::process::exit(1);
    }
    if !&rootfs.exists() {
        eprintln!("rootfs not exist");
        std::process::exit(1);
    }

    if appfs.is_some() && !appfs.as_ref().unwrap().exists(){
        eprintln!("appfs not exist");
        std::process::exit(1);
    }

    if dump_dir.is_some() && !dump_dir.as_ref().unwrap().exists(){
        eprintln!("dump directory not exist");
        std::process::exit(1);
    }

    for dir in &load_dir {
        if !dir.exists() {
            eprintln!("{:?} snapshot not exist", dir);
            std::process::exit(1);
        }
    }

    ts_vec.push(Instant::now());
    let json_dir = if let Some(dir) = load_dir.last() {
        Some(dir.clone())
    } else {
        None
    };
    let parsed_json = json_dir.map(|mut dir| {
        //println!("parsing meta data from {:?}", dir);
        dir.push("snapshot.json");
        let reader = BufReader::new(File::open(dir).expect("Failed to open snapshot.json"));
        serde_json::from_reader(reader).expect("Bad snapshot.json")
    });
    ts_vec.push(Instant::now());

    let from_snapshot = !load_dir.is_empty();
    let config = SnapFaaSConfig {
        parsed_json,
        load_dir,
        dump_dir,
        huge_page: false,
        base: MemoryFileOption { copy: copy_base, odirect: odirect_base},
        diff: MemoryFileOption { copy: copy_diff, odirect: odirect_diff},
        load_ws,
    };
    // Create vmm thread
    let mut vmm = match VmmWrapper::new(instance_id.clone(), config) {
        Ok(vmm) => vmm,
        Err(e) => {
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
        odirect: odirect_rootfs,
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
                odirect: odirect_appfs,
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

    if let Some(cid) = cid.clone() {
        let vsock_path = format!("worker-{}.sock", cid);
        let _ = std::fs::remove_file(&vsock_path);
        let vsock_config = VsockDeviceConfig {
            vsock_id: "vsock0".to_string(),
            guest_cid: cid,
            uds_path: vsock_path.to_string(),
        };
        if let Err(e) = vmm.add_vsock(vsock_config) {
            eprintln!("Vmm failed to add vsock due to: {:?}", e);
            std::process::exit(1);
        }
    }
 
    ts_vec.push(Instant::now());
    //TODO: Optionally add a logger

    // Launch vm
    if let Err(e) = vmm.start_instance() {
        eprintln!("Vmm failed to start instance due to: {:?}", e);
        std::process::exit(1);
    }

    // listen for dump working set
    if cmd_arguments.is_present("dump working set") {
        let listener_port = format!("dump_ws-{}.sock", instance_id);
        let unix_sock_listener = UnixListener::bind(listener_port).expect("Failed to bind to unix listener");
         match unix_sock_listener.accept() {
            Ok((_, _)) => match vmm.dump_working_set() {
                Ok(_) => {
                    eprintln!("VMM: dumped the working set.");
                    let port = format!("dump_ws-{}.sock.back", instance_id);
                    UnixStream::connect(port).expect("Failed to connect");
                },
                Err(e) => {
                    eprintln!("VMM: failed to dump the working set: {:?}", e);
                    std::process::exit(1);
                },
            },
            Err(e) => {
                eprintln!("VMM: failed to dump the working set: {:?}", e);
                std::process::exit(1);
            }
        }
    };
    let parse_time = ts_vec[2].duration_since(ts_vec[1]).as_micros();
    eprintln!("FR: Parse JSON: {} us\nFR: Preconfigure VM: {} us",
             parse_time,
             ts_vec[3].duration_since(ts_vec[0]).as_micros() - parse_time);
    vmm.join_vmm();
    std::process::exit(0);
}
