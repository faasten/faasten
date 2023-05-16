//! This binary is used to launch a single instance of firerunner
//! It reads a request from stdin, launches a VM based on cmdline inputs, sends
//! the request to VM, waits for VM's response and finally prints the response
//! to stdout, kills the VM and exits.
use labeled::buckle::{self, Buckle};
use log::{debug, error};
use snapfaas::blobstore::Blobstore;
use snapfaas::cli;
use snapfaas::configs::FunctionConfig;
use snapfaas::fs::tikv::TikvClient;
use snapfaas::fs::{BackingStore, FS};
use snapfaas::syscall_server::SyscallGlobalEnv;
use snapfaas::vm::Vm;
use snapfaas::{syscall_server, unlink_unix_sockets};
use std::io::BufRead;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    vmconfig: cli::VmConfig,
    /// If present, force singlevm to exit once firerunner exits
    #[arg(long, requires = "dump_snapshot")]
    force_exit: bool,
    /// Faasten ID
    #[arg(long)]
    login: Option<String>,
    /// Buckle label that the VM starts with
    #[arg(long, value_name = "BUCKLE")]
    start_label: Option<String>,
    #[command(flatten)]
    store: cli::Store,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    // Create a FunctionConfig value based on cmdline inputs
    let vm_app_config = FunctionConfig {
        mac: cli.vmconfig.mac,
        tap: cli.vmconfig.tap,
        runtimefs: cli.vmconfig.rootfs,
        appfs: cli.vmconfig.appfs,
        vcpus: cli.vmconfig.vcpu as u64,
        memory: cli.vmconfig.memory as usize,
        concurrency_limit: 1,
        load_dir: cli.vmconfig.load_dir,
        dump_dir: cli.vmconfig.dump_dir,
        copy_base: cli.vmconfig.copy_base_memory,
        copy_diff: cli.vmconfig.copy_diff_memory,
        kernel: cli.vmconfig.kernel,
        cmdline: cli.vmconfig.kernel_args,
        dump_ws: cli.vmconfig.dump_ws,
        load_ws: cli.vmconfig.load_ws,
    };

    let id = cli.vmconfig.id as usize;
    let odirect = snapfaas::vm::OdirectOption {
        base: cli.vmconfig.odirect_base,
        diff: cli.vmconfig.no_odirect_diff,
        rootfs: cli.vmconfig.no_odirect_root,
        appfs: cli.vmconfig.no_odirect_app,
    };

    // Launch a vm based on the FunctionConfig value
    let t1 = Instant::now();
    let mut vm = Vm::new(id, vm_app_config.clone().into());
    let vm_listener_path = format!("worker-{}.sock_1234", cli.vmconfig.vsock_cid);
    let _ = std::fs::remove_file(&vm_listener_path);
    let vm_listener = UnixListener::bind(vm_listener_path).expect("bind to the UNIX listener");
    let force_exit = cli.force_exit;
    if let Err(e) = vm.launch(
        vm_listener.try_clone().unwrap(),
        cli.vmconfig.vsock_cid,
        force_exit,
        vm_app_config,
        Some(odirect),
    ) {
        error!("VM launch failed: {:?}", e);
        snapfaas::unlink_unix_sockets();
    }
    let t2 = Instant::now();

    debug!("VM ready in: {} us", t2.duration_since(t1).as_micros());

    // create a vector of Request values from stdin
    let mut requests = Vec::new();
    let stdin = std::io::stdin();
    for line in std::io::BufReader::new(stdin).lines().map(|l| l.unwrap()) {
        requests.push(line);
    }
    let num_req = requests.len();
    let mut num_rsp = 0;

    let mypriv = cli
        .login
        .as_ref()
        .map_or(buckle::Component::dc_true(), |p| {
            Buckle::parse(&("T,".to_string() + p)).unwrap().integrity
        });
    let startlbl = cli
        .start_label
        .as_ref()
        .map_or(Buckle::public(), |s| Buckle::parse(s).unwrap());

    let fs: FS<Box<dyn BackingStore>> = if let Some(tikv_pds) = cli.store.tikv {
        FS::new({
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let client =
                rt.block_on(async { tikv_client::RawClient::new(tikv_pds, None).await.unwrap() });
            Box::new(TikvClient::new(client, Arc::new(rt)))
        })
    } else if let Some(path) = cli.store.lmdb.as_ref() {
        let dbenv = std::boxed::Box::leak(Box::new(snapfaas::fs::lmdb::get_dbenv(path)));
        FS::new(Box::new(&*dbenv))
    } else {
        panic!("We shouldn't reach here.");
    };

    let mut env = SyscallGlobalEnv {
        sched_conn: None,
        fs,
        blobstore: Blobstore::default(),
    };

    // Synchronously send the request to vm and wait for a response
    let dump_working_set = true && cli.vmconfig.dump_ws;
    for req in requests {
        let t1 = Instant::now();
        debug!("request: {:?}", req);
        let processor =
            syscall_server::SyscallProcessor::new(startlbl.clone(), mypriv.clone(), Buckle::top());
        match processor.run(&mut env, req, &mut vm) {
            Ok(rsp) => {
                let t2 = Instant::now();
                eprintln!(
                    "request returned in: {} us",
                    t2.duration_since(t1).as_micros()
                );
                println!("{}", rsp.payload.unwrap_or(String::from("")));
                num_rsp += 1;
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
    eprintln!("***********************************************");
    eprintln!("Total requests: {}, Total resposnes: {}", num_req, num_rsp);
    eprintln!("***********************************************");

    // Shutdown the vm and exit
    eprintln!("Shutting down vm...");
    drop(vm);
    unlink_unix_sockets();
}
