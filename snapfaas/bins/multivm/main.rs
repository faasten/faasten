//! FIXME update this comment The SnapFaaS Controller
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
use r2d2::Pool;
use snapfaas::resource_manager::ResourceManager;
use snapfaas::sched::{self, Scheduler};
use snapfaas::worker::Worker;

use std::sync::{Arc, Mutex};

fn main() {
    env_logger::init();

    let matches = App::new("SnapFaaS controller")
        .version("0.1")
        .about("Worker machine local manager")
        //.arg(
        //    Arg::with_name("config")
        //        .value_name("YAML")
        //        .short("c")
        //        .long("config")
        //        .takes_value(true)
        //        .required(true)
        //        .help("Path to controller config YAML file"),
        //)
        .arg(
            Arg::with_name("scheduler address")
                .value_name("[ADDR:]PORT")
                .long("scheduler")
                .short("s")
                .takes_value(true)
                .required(true)
                .help("Address on which SnapFaaS listen for RPCs that requests for tasks"),
        )
        .arg(
            Arg::with_name("total memory")
                .value_name("MB")
                .long("mem")
                .takes_value(true)
                .required(true)
                .help("Total memory available for all VMs"),
        )
        .get_matches();

    let sched_addr = matches.value_of("scheduler address").unwrap();

    let mut manager = ResourceManager::new();
    let total_mem = matches
        .value_of("total memory")
        .unwrap()
        .parse::<usize>()
        .expect("Total memory is not a valid integer");
    manager.set_total_mem(total_mem);

    // create the worker pool
    let pool_size = manager.total_mem_in_mb() / 128;
    let conn = Pool::builder()
        .max_size(pool_size as u32 + 1)
        .build(Scheduler::new(sched_addr))
        .unwrap();
    let pool = threadpool::ThreadPool::new(pool_size);
    let manager = Arc::new(Mutex::new(manager));
    for i in 0..pool_size as u32 {
        let manager_dup = Arc::clone(&manager);
        let conn_dup = conn.clone();
        pool.execute(move || {
            Worker::new(i + 100, conn_dup, manager_dup).wait_and_process();
        });
    }

    // register signal handler
    set_ctrlc_handler(conn);

    // hold on
    pool.join();
}

//fn new_workerpool(
//    pool_size: usize, sched_addr: String, manager_sender: Sender<Message>
//) -> Vec<Worker> {
//    let mut pool = Vec::with_capacity(pool_size);
//    for i in 0..pool_size {
//        let cid = i as u32 + 100;
//        pool.push(Worker::new(
//            sched_addr.clone(),
//            manager_sender.clone(),
//            cid,
//        ));
//    }
//    pool
//}

fn set_ctrlc_handler(conn: Pool<Scheduler>) {
    ctrlc::set_handler(move || {
        warn!("{}", "Handling Ctrl-C. Shutting down...");
        if let Ok(mut sched) = conn.get() {
            let _ = sched::rpc::drop_resource(&mut sched);
        }
        snapfaas::unlink_unix_sockets();
        std::process::exit(0);
    })
    .expect("set Ctrl-C handler");
}
