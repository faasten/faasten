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
use snapfaas::resource_manager::ResourceManager;
use snapfaas::{sched, fs::lmdb::DBENV, fs::tikv::TikvClient, fs::BackingStore};
use snapfaas::worker::Worker;

use std::net::{SocketAddr, TcpStream};
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
        .arg(
            Arg::with_name("tikv proxies")
                .value_name("[ADDR:]PORT")
                .long("tikv")
                .takes_value(true)
                .required(false)
                .help("One or more addresses of TiKV placement driver, separated by space.")
        )
        .get_matches();

    // intialize remote scheduler
    let sched_addr = matches
        .value_of("scheduler address")
        .map(String::from)
        .unwrap();
    let sched_addr = sched_addr
        .parse::<SocketAddr>()
        .expect("invalid socket address");

    // create the local resource manager
    let mut manager = ResourceManager::new(sched_addr.clone());

    // set total memory
    let total_mem = matches
        .value_of("total memory")
        .unwrap()
        .parse::<usize>()
        .expect("Total memory is not a valid integer");
    manager.set_total_mem(total_mem);

    // create the worker pool
    let pool_size = manager.total_mem_in_mb() / 128;
    let pool = match matches.value_of("tikv proxies").map(|ts| ts.split_whitespace().into_iter().collect()) {
        None => {
            new_workerpool(pool_size, sched_addr, manager, &*DBENV)
        }
        Some(tikv_pds) => {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let client = rt.block_on(async { tikv_client::RawClient::new(tikv_pds, None).await.unwrap() });
            let db = TikvClient::new(client, Arc::new(rt));
            new_workerpool(pool_size, sched_addr, manager, db)
        }
    };

    // register signal handler
    set_ctrlc_handler(sched_addr.clone());

    // hold on
    pool.join();
}

fn new_workerpool<T>(
    pool_size: usize,
    sched_addr: SocketAddr,
    manager: ResourceManager,
    db: T
) -> threadpool::ThreadPool
where
    T: BackingStore + Clone + Send + 'static
{
    let pool = threadpool::ThreadPool::new(pool_size);
    let manager = Arc::new(Mutex::new(manager));
    for i in 0..pool_size as u32 {
        let sched_addr_dup = sched_addr.clone();
        let manager_dup = Arc::clone(&manager);
        let db_dup = db.clone();
        pool.execute(move || {
            Worker::new(i + 100, sched_addr_dup, manager_dup, db_dup).wait_and_process();
        });
    }
    pool
}

fn set_ctrlc_handler(sched_addr: SocketAddr) {
    ctrlc::set_handler(move || {
        warn!("{}", "Handling Ctrl-C. Shutting down...");
        if let Ok(mut sched) = TcpStream::connect(sched_addr.clone()) {
            let _ = sched::rpc::drop_resource(&mut sched);
        }
        snapfaas::unlink_unix_sockets();
        std::process::exit(0);
    })
    .expect("set Ctrl-C handler");
}
