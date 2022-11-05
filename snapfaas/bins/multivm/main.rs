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
use snapfaas::message::Message;
use snapfaas::worker::Worker;
use snapfaas::sched;
use snapfaas::sched::gateway::Gateway;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::thread::{JoinHandle, self};

fn main() {
    env_logger::init();

    let matches = App::new("SnapFaaS controller")
        .version("1.0")
        .author("David H. Liu <hao.liu@princeton.edu>")
        .about("Launch and configure SnapFaaS controller")
        .arg(Arg::with_name("mock github")
            .long("mock_github")
            .value_name("MOCK GITHUB ADDRESS")
            .help("If present, use the mock GitHub service at the supplied address.")
        )
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
            Arg::with_name("http listen address")
                .value_name("[ADDR:]PORT")
                .long("listen_http")
                .short("l")
                .takes_value(true)
                .required(true)
                .help("Address on which SnapFaaS listen for connections that sends requests"),
        )
        .arg(
            Arg::with_name("scheduler listen address")
                .value_name("[ADDR:]PORT")
                .long("listen_sched")
                .short("s")
                .takes_value(true)
                .required(true)
                .help("Address on which SnapFaaS listen for RPCs that requests for tasks"),
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

    // create the local resource manager
    let (mut manager, manager_sender) = ResourceManager::new(config);

    // set total memory
    let total_mem = matches
                        .value_of("total memory")
                        .unwrap()
                        .parse::<usize>()
                        .expect("Total memory is not a valid integer");
    manager.set_total_mem(total_mem);

    // intialize remote scheduler
    let sched_addr = matches
                        .value_of("scheduler listen address")
                        .map(String::from)
                        .unwrap();
    let sched_resman =
        Arc::new(Mutex::new(
            sched::resource_manager::ResourceManager::new()
        ));
    let mut schedgate = sched::gateway::SchedGateway::listen(
        &sched_addr, Some(Arc::clone(&sched_resman))
    );

    // create the worker pool
    let mock_github = matches.value_of("mock github").map(String::from);
    let pool = new_workerpool(
                    manager.total_mem()/128,
                    sched_addr.clone(),
                    manager_sender.clone(),
                    mock_github,
                );
    // kick off the local resource manager
    let manager_handle = manager.run();

    // register signal handler
    set_ctrlc_handler(pool, sched_addr.clone(), manager_sender, Some(manager_handle));

    // TCP gateway
    if let Some(l) = matches.value_of("http listen address") {
        let gateway = sched::gateway::HTTPGateway::listen(l, None);
        for (request, _timestamps) in gateway {
            // Return when a VM acquisition succeeds or fails
            // but before a VM launches (if it is newly allocated)
            // and execute the request.
            if let Some(_) = schedgate.next() {
                let sched_resman_dup = Arc::clone(&sched_resman);
                thread::spawn(move || {
                    let _ = sched::schedule(request, sched_resman_dup);
                });
            }
        }
    }
}

fn new_workerpool(
    pool_size: usize, sched_addr: String,
    manager_sender: Sender<Message>, mock_github: Option<String>
) -> Vec<Worker> {
// ) -> (Vec<Worker>, Sender<Message>) {
    // let (request_sender, response_receiver) = mpsc::channel();
    // let response_receiver = Arc::new(Mutex::new(response_receiver));
    let mut pool = Vec::with_capacity(pool_size);
    for i in 0..pool_size {
        let cid = i as u32 + 100;
        pool.push(Worker::new(
            // response_receiver.clone(),
            manager_sender.clone(),
            // request_sender.clone(),
            cid, mock_github.clone(),
            sched_addr.clone(),
        ));
    }

    pool
    // (pool, request_sender)
}

fn set_ctrlc_handler(
    mut pool: Vec<Worker>, sched_addr: String,
    manager_sender: Sender<Message>, mut manager_handle: Option<JoinHandle<()>>
) {
    ctrlc::set_handler(move || {
        println!("ctrlc handler");
        warn!("{}", "Handling Ctrl-C. Shutting down...");
        let _ = sched::Scheduler::connect(&sched_addr)
            .shutdown_all();
        while let Some(worker) = pool.pop() {
            worker.join().expect("failed to join worker thread");
        }
        snapfaas::unlink_unix_sockets();
        manager_sender.send(Message::Shutdown).expect("failed to shut down resource manager");
        manager_handle.take().map(JoinHandle::join).unwrap().expect("failed to join resource manager thread");
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
}
