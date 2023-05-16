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

use clap::Parser;
use log::warn;
use snapfaas::cli;
use snapfaas::resource_manager::ResourceManager;
use snapfaas::worker::Worker;
use snapfaas::{fs::tikv::TikvClient, fs::BackingStore, sched};

use std::net::{SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Address of the scheduler
    #[arg(short, long, value_name = "ADDR:PORT")]
    scheduler: String,
    /// Total memory in MBs of the worker machine
    #[arg(short, long, value_name="MB", value_parser=clap::value_parser!(u32).range(128..))]
    memory: u32,
    #[command(flatten)]
    store: cli::Store,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    // create the local resource manager
    let sched_addr: SocketAddr =
        SocketAddr::from_str(&cli.scheduler).expect("Invalid socket address");
    let mut manager = ResourceManager::new(sched_addr.clone());

    // set total memory
    manager.set_total_mem(cli.memory as usize);

    // create the worker pool
    let pool_size = manager.total_mem_in_mb() / 128;
    let pool = if let Some(path) = cli.store.lmdb.as_ref() {
        let dbenv = std::boxed::Box::leak(Box::new(snapfaas::fs::lmdb::get_dbenv(path)));
        new_workerpool(pool_size, sched_addr, manager, &*dbenv)
    } else if let Some(tikv_pds) = cli.store.tikv {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let client =
            rt.block_on(async { tikv_client::RawClient::new(tikv_pds, None).await.unwrap() });
        let db = TikvClient::new(client, Arc::new(rt));
        new_workerpool(pool_size, sched_addr, manager, db)
    } else {
        panic!("We shouldn't reach here");
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
    db: T,
) -> threadpool::ThreadPool
where
    T: BackingStore + Clone + Send + 'static,
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
