//! The SnapFaaS Controller
//!
//! The Controller consists of a request manager (file or HTTP) and a pool of workers.
//! The gateway takes in requests. The controller assigns each request a worker.
//! Each worker is responsible for finding a VM to handle the request and proxies the response.
//!
//! The Controller maintains several states:
//!   1. kernel path
//!   2. kernel boot argument
//!   3. function store and their files' locations

use clap::{App, Arg};
use log::warn;
use snapfaas::configs;
use snapfaas::resource_manager::ResourceManager;
use snapfaas::gateway;
use snapfaas::message::Message;
use snapfaas::worker::Worker;

use std::sync::{mpsc, Arc, Mutex};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

fn main() {
    env_logger::init();

    let matches = App::new("SnapFaaS controller")
        .version("1.0")
        .author("David H. Liu <hao.liu@princeton.edu>")
        .about("Launch and configure SnapFaaS controller")
        .arg(
            Arg::with_name("config")
                .value_name("YAML")
                .short("c")
                .long("config")
                .takes_value(true)
                .required(true)
                .help("Path to controller config YAML file"),
        )
        .arg(
            Arg::with_name("listen address")
                .value_name("[ADDR:]PORT")
                .long("listen")
                .short("l")
                .takes_value(true)
                .required(true)
                .help("Address on which SnapFaaS listen for connections that sends requests"),
        )
        .arg(Arg::with_name("total memory")
                .value_name("MB")
                .long("mem")
                .takes_value(true)
                .required(true)
                .help("Total memory available for all VMs")
        )
        .get_matches();

    // populate the in-memory config struct
    let config_path = matches.value_of("config").unwrap();
    let config = configs::ResourceManagerConfig::new(config_path);

    let fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    fs.initialize();
    if let Ok(snapfaas::fs::DirEntry::Directory(root)) = snapfaas::fs::utils::read_path(&fs, &vec![]) {
        // set up home directories
        let fdir = fs.create_faceted_directory();
        let _ = fs.link(&root, "home".to_string(), snapfaas::fs::DirEntry::FacetedDirectory(fdir));
        // TODO: for now, set up gates for functions in the configuration directly under the root
        // with empty privilege and no invoking restriction.
        for image in config.functions.keys() {
            let gate = fs.create_gate(true.into(), true.into(), image.clone()).unwrap();
            let _ = fs.link(&root, image.clone(), snapfaas::fs::DirEntry::Gate(gate));
        }
    } else {
        panic!("Failed to read the root");
    }

    // create the resource manager
    let (mut manager, manager_sender) = ResourceManager::new(config);

    // set total memory
    let total_mem = matches.value_of("total memory").unwrap()
        .parse::<usize>().expect("Total memory is not a valid integer");
    manager.set_total_mem(total_mem);

    // create the worker pool
    let (pool, request_sender) = new_workerpool(manager.total_mem()/128, manager_sender.clone());
    // kick off the resource manager
    let manager_handle = manager.run();

    // register signal handler
    set_ctrlc_handler(request_sender.clone(), pool, manager_sender, Some(manager_handle));

    // TCP gateway
    if let Some(l) = matches.value_of("listen address") {
        let gateway = gateway::HTTPGateway::listen(l);

        for (request, response_tx, timestamps) in gateway {
            // Return when a VM acquisition succeeds or fails
            // but before a VM launches (if it is newly allocated)
            // and execute the request.
            request_sender.send(Message::Request((request, response_tx, timestamps))).expect("Failed to send request");
        }
    }
}

fn new_workerpool(pool_size: usize, manager_sender: Sender<Message>) -> (Vec<Worker>, Sender<Message>) {
    let (request_sender, response_receiver) = mpsc::channel();
    let response_receiver = Arc::new(Mutex::new(response_receiver));

    let mut pool = Vec::with_capacity(pool_size);

    for i in 0..pool_size {
        let cid = i as u32 + 100;
        pool.push(Worker::new(response_receiver.clone(), manager_sender.clone(), request_sender.clone(), cid));
    }

    (pool, request_sender)
}

fn set_ctrlc_handler(request_sender: Sender<Message>, mut pool: Vec<Worker>, manager_sender: Sender<Message>, mut manager_handle: Option<JoinHandle<()>>) {
    ctrlc::set_handler(move || {
        println!("");
        warn!("{}", "Handling Ctrl-C. Shutting down...");
        let pool_size = pool.len();
        for _ in 0..pool_size {
            request_sender.send(Message::Shutdown).expect("failed to shut down workers");
        }
        while let Some(worker) = pool.pop() {
            worker.join().expect("failed to join worker thread");
        }
        snapfaas::unlink_unix_sockets();
        manager_sender.send(Message::Shutdown).expect("failed to shut down resource manager");
        manager_handle.take().map(JoinHandle::join).unwrap().expect("failed to join resource manager thread");
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
}
