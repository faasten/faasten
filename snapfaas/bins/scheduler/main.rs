use clap::{crate_authors, crate_version, App, Arg};

use std::{
    sync::{Arc, Condvar, Mutex},
    thread,
};

use snapfaas::sched::{resource_manager::ResourceManager, rpc_server::RpcServer, schedule};

fn main() {
    env_logger::init();

    let matches = App::new("Faasten Gateway")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Launch Faasten gateway")
        .arg(
            Arg::with_name("scheduler listen address")
                .value_name("[ADDR:]PORT")
                .long("listen")
                .short("s")
                .takes_value(true)
                .required(true)
                .help("Address on which Faasten listen for RPCs that requests for tasks"),
        )
        .arg(
            Arg::with_name("queue capacity")
                .value_name("CAP_NUM_OF_TASK")
                .long("qcap")
                .takes_value(true)
                .required(true)
                .default_value("1000000")
                .help("Address on which Faasten listen for RPCs that requests for tasks"),
        )
        .get_matches();

    // Start garbage collector
    thread::spawn(|| {
        use snapfaas::fs;
        loop {
            thread::sleep(std::time::Duration::new(5, 0));
            fs::utils::taint_with_label(labeled::buckle::Buckle::top());
            let mut fs = fs::FS::new(&*snapfaas::fs::lmdb::DBENV);
            let collected = fs.collect_garbage().unwrap();
            log::debug!("garbage collected {}", collected.len())
        }
    });

    // Intialize remote scheduler
    let sched_addr = matches
        .value_of("scheduler listen address")
        .map(String::from)
        .unwrap();
    let qcap = matches
        .value_of("queue capacity")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let (queue_tx, queue_rx) = crossbeam::channel::bounded(qcap);
    let manager = Arc::new(Mutex::new(ResourceManager::new()));
    let cvar = Arc::new(Condvar::new());

    // Register signal handler
    set_ctrlc_handler(manager.clone());

    // kick off scheduling thread
    let manager_dup = manager.clone();
    let cvar_dup = cvar.clone();
    thread::spawn(move || schedule(queue_rx, manager_dup, cvar_dup));

    let s = RpcServer::new(&sched_addr, manager.clone(), queue_tx, cvar);
    log::debug!("Scheduler starts listening at {:?}", sched_addr);
    s.run();
}

fn set_ctrlc_handler(manager: Arc<Mutex<ResourceManager>>) {
    ctrlc::set_handler(move || {
        log::warn!("{}", "Handling Ctrl-C. Shutting down...");
        manager.lock().unwrap().remove_all();
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");
}
