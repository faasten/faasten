use clap::{App, Arg, crate_authors, crate_version};
use log::warn;
use snapfaas::configs;
use snapfaas::resource_manager::ResourceManager;
use snapfaas::message::Message;
use snapfaas::worker::Worker;
use snapfaas::sched;

use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

fn main() {
    env_logger::init();

    let matches = App::new("Faasten Worker")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Launch and configure Faasten worker node")
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
    let sched_addr = matches
                        .value_of("scheduler listen address")
                        .map(String::from)
                        .unwrap();
    let (mut manager, manager_sender) = ResourceManager::new(config, sched_addr.clone());

    // set total memory
    let total_mem = matches
                        .value_of("total memory")
                        .unwrap()
                        .parse::<usize>()
                        .expect("Total memory is not a valid integer");
    manager.set_total_mem(total_mem);

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

    // hold on
    let (_, rx) = mpsc::channel::<usize>();
    loop { let _ = rx.recv(); }
}

fn new_workerpool(
    pool_size: usize, sched_addr: String,
    manager_sender: Sender<Message>, mock_github: Option<String>
) -> Vec<Worker> {
    let mut pool = Vec::with_capacity(pool_size);
    for i in 0..pool_size {
        let cid = i as u32 + 100;
        pool.push(Worker::new(
            manager_sender.clone(),
            cid, mock_github.clone(),
            sched_addr.clone(),
        ));
    }
    pool
}

fn set_ctrlc_handler(
    mut pool: Vec<Worker>, sched_addr: String,
    manager_sender: Sender<Message>, mut manager_handle: Option<JoinHandle<()>>
) {
    ctrlc::set_handler(move || {
        warn!("{}", "Handling Ctrl-C. Shutting down...");
        let sched = sched::Scheduler::new(sched_addr.clone());
        let _ = sched.shutdown_all();
        while let Some(worker) = pool.pop() {
            worker.join().expect("failed to join worker thread");
        }
        snapfaas::unlink_unix_sockets();
        manager_sender.send(Message::Shutdown).expect("failed to shut down resource manager");
        manager_handle.take().map(JoinHandle::join).unwrap().expect("failed to join resource manager thread");
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
}
