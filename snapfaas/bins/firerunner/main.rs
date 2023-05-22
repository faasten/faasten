use std::fs::File;
use std::io::BufReader;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Instant;

use memory_model::MemoryFileOption;
use net_util::MacAddr;
use vmm::vmm_config::boot_source::BootSourceConfig;
use vmm::vmm_config::drive::BlockDeviceConfig;
use vmm::vmm_config::machine_config::VmConfig;
use vmm::vmm_config::net::NetworkInterfaceConfig;
use vmm::vmm_config::vsock::VsockDeviceConfig;
use vmm::SnapFaaSConfig;

use clap::Parser;

use snapfaas::cli;
use snapfaas::firecracker_wrapper::VmmWrapper;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    vmconfig: cli::VmConfig,
}

fn main() {
    let mut ts_vec = Vec::with_capacity(10);
    ts_vec.push(Instant::now());

    let args = Cli::parse().vmconfig;

    // process command line arguments
    let instance_id = args.id;
    let kernel = PathBuf::from(args.kernel);
    let rootfs = PathBuf::from(args.rootfs);
    let kargs = args.kernel_args;
    let mem_size_mib = args.memory as usize;
    let vcpu_count = args.vcpu as usize;

    // optional arguments:
    let appfs = args.appfs.map(PathBuf::from);
    let dump_dir = args.dump_dir.map(PathBuf::from);
    let load_dir = args.load_dir.map_or(Vec::new(), |x| {
        x.split(',')
            .collect::<Vec<&str>>()
            .iter()
            .map(PathBuf::from)
            .collect()
    });
    let copy_base = args.copy_base_memory;
    let copy_diff = args.copy_diff_memory;
    let odirect_base = args.odirect_base;
    let odirect_diff = !args.no_odirect_diff;
    let odirect_rootfs = !args.no_odirect_root;
    let odirect_appfs = !args.no_odirect_app;
    let load_ws = args.load_ws;
    let mac = args.mac;
    let tap_name = args.tap;
    let cid = args.vsock_cid;

    // Make sure kernel, rootfs, appfs, load_dir, dump_dir exist
    if !&kernel.exists() {
        eprintln!("kernel not exist");
        std::process::exit(1);
    }
    if !&rootfs.exists() {
        eprintln!("rootfs not exist");
        std::process::exit(1);
    }

    if appfs.is_some() && !appfs.as_ref().unwrap().exists() {
        eprintln!("appfs not exist");
        std::process::exit(1);
    }

    if dump_dir.is_some() && !dump_dir.as_ref().unwrap().exists() {
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
        base: MemoryFileOption {
            copy: copy_base,
            odirect: odirect_base,
        },
        diff: MemoryFileOption {
            copy: copy_diff,
            odirect: odirect_diff,
        },
        load_ws,
    };
    // Create vmm thread
    let mut vmm = match VmmWrapper::new(instance_id.to_string(), config) {
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
    let machine_config = VmConfig {
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
            boot_args: kargs,
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

    ts_vec.push(Instant::now());
    //TODO: Optionally add a logger

    // Launch vm
    if let Err(e) = vmm.start_instance() {
        eprintln!("Vmm failed to start instance due to: {:?}", e);
        std::process::exit(1);
    }

    // listen for dump working set
    if args.dump_ws {
        let listener_port = format!("dump_ws-{}.sock", instance_id);
        let unix_sock_listener =
            UnixListener::bind(listener_port).expect("Failed to bind to unix listener");
        match unix_sock_listener.accept() {
            Ok((_, _)) => match vmm.dump_working_set() {
                Ok(_) => {
                    eprintln!("VMM: dumped the working set.");
                    let port = format!("dump_ws-{}.sock.back", instance_id);
                    UnixStream::connect(port).expect("Failed to connect");
                }
                Err(e) => {
                    eprintln!("VMM: failed to dump the working set: {:?}", e);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("VMM: failed to dump the working set: {:?}", e);
                std::process::exit(1);
            }
        }
    };
    let parse_time = ts_vec[2].duration_since(ts_vec[1]).as_micros();
    eprintln!(
        "FR: Parse JSON: {} us\nFR: Preconfigure VM: {} us",
        parse_time,
        ts_vec[3].duration_since(ts_vec[0]).as_micros() - parse_time
    );
    vmm.join_vmm();
    std::process::exit(0);
}
