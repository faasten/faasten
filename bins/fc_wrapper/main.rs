#[macro_use(crate_version, crate_authors)]
extern crate clap;
/// This binary is used to launch a single instance of firerunner
/// It reads a request from stdin, launches a VM based on cmdline inputs, sends
/// the request to VM, waits for VM's response and finally prints the response
/// to stdout, kills the VM and exits.
use snapfaas::vm::Vm;
use snapfaas::request;
use snapfaas::configs::FunctionConfig;
use std::io::BufRead;
use snapfaas::vsock::{VsockListener, VMADDR_CID_HOST, VSOCKPORT};

use clap::{App, Arg};

fn main() {
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
                .required(true)
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
            Arg::with_name("network")
                .long("network")
                .value_name("NETWORK")
                .takes_value(true)
                .required(false)
                .help("newtork device of format TAP_NAME/MAC_ADDRESS")
        )
        .get_matches();

    // Create a FunctionConfig value based on cmdline inputs
    let vm_app_config = FunctionConfig {
        name: "app".to_string(), //dummy value
        runtimefs: cmd_arguments.value_of("rootfs").expect("rootfs").to_string(),
        appfs: cmd_arguments.value_of("appfs").expect("appfs").to_string(),
        vcpus: cmd_arguments.value_of("vcpu_count").expect("vcpu")
                            .parse::<u64>().expect("vcpu not int"),
        memory: cmd_arguments.value_of("mem_size").expect("mem_size")
                            .parse::<usize>().expect("mem_size not int"),
        concurrency_limit: 1,
        load_dir: cmd_arguments.value_of("load_dir").map(|s| s.to_string()),
        dump_dir: cmd_arguments.value_of("dump_dir").map(|s| s.to_string()),
        copy_base: cmd_arguments.is_present("copy_base_memory"),
        copy_diff: cmd_arguments.is_present("copy_diff_memory"),
        hugepage: false,
        kernel: cmd_arguments.value_of("kernel").expect("kernel").to_string(),
        cmdline: Some(cmd_arguments.value_of("kernel_args").expect("kernel_args").to_string()),
    };
    let id: &str = cmd_arguments.value_of("id").expect("id");
    println!("id: {}, function config: {:?}", id, vm_app_config);

    let mut listener = VsockListener::bind(VMADDR_CID_HOST, VSOCKPORT).expect("bind");
    let (sender, receiver) = std::sync::mpsc::channel();
    let listener_thread = std::thread::spawn(move || {
        let (conn, _) = listener.accept().expect("accept");
        sender.send(conn).expect("failed to send vsock stream");
        listener.closer().close();
    });
    // Launch a vm based on the FunctionConfig value
    let mut vm = match Vm::new(id, &vm_app_config, &receiver, 124, cmd_arguments.value_of("network")) {
        Ok(vm) => vm,
        Err(e) => {
            println!("Vm creation failed due to: {:?}", e);
            std::process::exit(1);
        }
    };

    println!("Vm creation succeeded");

    // create a vector of Request values from stdin
    let mut requests: Vec<request::Request> = Vec::new();
    let stdin = std::io::stdin();
    for line in std::io::BufReader::new(stdin).lines().map(|l| l.unwrap()) {
        match request::parse_json(&line) {
            Ok(j) => {
                requests.push(j);
            }
            Err(e) => {
                println!("invalid requests: {:?}", e);
                std::process::exit(1);
            }
        }
    }

    let num_req = requests.len();
    let mut num_rsp = 0;

    // Synchronously send the request to vm and wait for a response
    for req in requests {
        match vm.process_req(req) {
            Ok(rsp) => {
                println!("Request succeeded: {:?}", rsp);
                num_rsp+=1;
            }
            Err(e) => {
                println!("Request failed due to: {:?}", e);
            }
        }
    }

    println!("***********************************************");
    println!("Total requests: {}, Total resposnes: {}", num_req, num_rsp);
    println!("***********************************************");

    // Shutdown the vm and exit
    println!("Shutting down vm...");
    listener_thread.join().expect("join");
    vm.shutdown();
}
