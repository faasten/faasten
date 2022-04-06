#[macro_use(crate_version, crate_authors)]
extern crate clap;
/// This binary is used to launch a single instance of firerunner
/// It reads a request from stdin, launches a VM based on cmdline inputs, sends
/// the request to VM, waits for VM's response and finally prints the response
/// to stdout, kills the VM and exits.
use snapfaas::vm::Vm;
use snapfaas::unlink_unix_sockets;
use snapfaas::configs::FunctionConfig;
use std::io::{BufRead};
use std::sync::{Arc, Mutex};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Instant;
use log;
use serde_json;

use clap::{App, Arg};

const CID: u32 = 100;
// If firerunner path is not set by the user, the program will assume firerunner is on the
// environment variable PATH.
const DEFAULT_FIRERUNNER: &str = "firerunner";

fn main() {
    env_logger::init();
    let cmd_arguments = App::new("fireruner wrapper")
        .version(crate_version!())
        .author(crate_authors!())
        .about("launch a single firerunner vm.")
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
                .help("path to the app file system")
        )
        .arg(
            Arg::with_name("id")
                .long("id")
                .help("microvm unique identifier")
                .default_value("1234")
                .required(true)
                .takes_value(true)
        )
        .arg(
            Arg::with_name("load_dir")
                .long("load_dir")
                .takes_value(true)
                .required(false)
                .help("if specified start vm from a snapshot under the given directory")
        )
        .arg(
            Arg::with_name("dump_dir")
                .long("dump_dir")
                .takes_value(true)
                .required(false)
                .help("if specified creates a snapshot right after runtime is up under the given directory")
        )
        .arg(
            Arg::with_name("mem_size")
                 .long("mem_size")
                 .value_name("MB")
                 .default_value("128")
                 .help("Guest memory size in MB (default is 128)")
        )
        .arg(
            Arg::with_name("vcpu_count")
                 .long("vcpu_count")
                 .value_name("VCPUCOUNT")
                 .default_value("1")
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
                 .takes_value(false)
                 .required(false)
                 .help("If a diff snapshot is provided, restore its memory by copying")
        )
        .arg(
            Arg::with_name("enable network")
                .long("network")
                .takes_value(false)
                .required(false)
                .help("enable newtork through tap device `tap0`")
        )
        .arg(
            Arg::with_name("firerunner")
                .long("firerunner")
                .value_name("PATH_TO_FIRERUNNER")
                .default_value(DEFAULT_FIRERUNNER)
                .help("path to the firerunner binary")
        )
        .arg(
            Arg::with_name("force exit")
                .long("force_exit")
                .value_name("FORCEEXIT")
                .takes_value(false)
                .required(false)
                .help("force fc_wrapper to exit once firerunner exits")
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
                .help("If present, VMM will send `dump working set` action to the VM when the host signals through the unix socket connection. The value is the directory to put the working set in")
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

    if cmd_arguments.is_present("enable network") {
        // warn if tap0 is missing the program will hang
        log::warn!("Network turned on. Tap device `tap0` must be present.");
    }

    // Create a FunctionConfig value based on cmdline inputs
    let vm_app_config = FunctionConfig {
        network: cmd_arguments.is_present("enable network"),
        runtimefs: cmd_arguments.value_of("rootfs").expect("rootfs").to_string(),
        appfs: cmd_arguments.value_of("appfs").map(|s| s.to_string()),
        vcpus: cmd_arguments.value_of("vcpu_count").expect("vcpu")
                            .parse::<u64>().expect("vcpu not int"),
        memory: cmd_arguments.value_of("mem_size").expect("mem_size")
                            .parse::<usize>().expect("mem_size not int"),
        concurrency_limit: 1,
        load_dir: cmd_arguments.value_of("load_dir").map(|s| s.to_string()),
        dump_dir: cmd_arguments.value_of("dump_dir").map(|s| s.to_string()),
        copy_base: cmd_arguments.is_present("copy_base_memory"),
        copy_diff: cmd_arguments.is_present("copy_diff_memory"),
        kernel: cmd_arguments.value_of("kernel").expect("kernel").to_string(),
        cmdline: cmd_arguments.value_of("kernel_args").map(|s| s.to_string()),
        dump_ws: cmd_arguments.is_present("dump working set"),
        load_ws: cmd_arguments.is_present("load working set"),
    };
    let id = cmd_arguments.value_of("id").unwrap().parse::<usize>().unwrap();
    let odirect = snapfaas::vm::OdirectOption {
        base: cmd_arguments.is_present("odirect base"),
        diff: !cmd_arguments.is_present("no odirect diff"),
        rootfs: !cmd_arguments.is_present("no odirect rootfs"),
        appfs: !cmd_arguments.is_present("no odirect appfs")
    };
    let firerunner = cmd_arguments.value_of("firerunner").unwrap().to_string();
    let allow_network = cmd_arguments.is_present("enable network");

    // Launch a vm based on the FunctionConfig value
    let t1 = Instant::now();
    let mut vm =  Vm::new(id, firerunner, "myapp".to_string(), vm_app_config, allow_network, Arc::new(Mutex::new(snapfaas::labeled_fs::LabeledFS::new())));
    let vm_listener_path = format!("worker-{}.sock_1234", CID);
    let vm_listener = UnixListener::bind(vm_listener_path).expect("Failed to bind to unix listener");
    let force_exit = cmd_arguments.is_present("force_exit");
    if let Err(e) = vm.launch(None, vm_listener, CID, force_exit, Some(odirect)) {
        log::error!("unable to launch the VM: {:?}", e);
        snapfaas::unlink_unix_sockets();
    }
    let t2 = Instant::now();

    println!("VM ready in: {} us", t2.duration_since(t1).as_micros());

    // create a vector of Request values from stdin
    let mut requests: Vec<serde_json::Value> = Vec::new();
    let stdin = std::io::stdin();
    for line in std::io::BufReader::new(stdin).lines().map(|l| l.unwrap()) {
        match serde_json::from_str(&line) {
            Ok(j) => {
                requests.push(j);
            }
            Err(e) => {
                eprintln!("invalid requests: {:?}", e);
                drop(vm);
                unlink_unix_sockets();
                std::process::exit(1);
            }
        }
    }
    let num_req = requests.len();
    let mut num_rsp = 0;

    // Synchronously send the request to vm and wait for a response
    let dump_working_set = true && cmd_arguments.is_present("dump working set");
    for req in requests {
        let t1 = Instant::now();
        log::debug!("request: {:?}", req);
        match vm.process_req(req) {
            Ok(rsp) => {
                let t2 = Instant::now();
                println!("request returned in: {} us", t2.duration_since(t1).as_micros());
                log::debug!("response: {:?}", rsp);
                num_rsp+=1;
            }
            Err(e) => {
                eprintln!("Request failed due to: {:?}", e);
            }
        }
        if dump_working_set {
            let listener_port = format!("dump_ws-{}.sock", id);
            UnixStream::connect(listener_port).expect("Failed to connect to VMM UNIX listener");
            let port = format!("dump_ws-{}.sock.back", id);
            let li = UnixListener::bind(port).expect("Failed to listen at the port");
            li.accept().expect("Failed to accept a connection");
            break;
        }
    }


    println!("***********************************************");
    println!("Total requests: {}, Total resposnes: {}", num_req, num_rsp);
    println!("***********************************************");

    // Shutdown the vm and exit
    println!("Shutting down vm...");
    drop(vm);
    unlink_unix_sockets();
}
