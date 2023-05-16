use clap::Parser;

use std::{
    sync::{Arc, Condvar, Mutex},
    thread,
};

use snapfaas::sched::{resource_manager::ResourceManager, rpc_server::RpcServer, schedule};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Address to listen at
    #[arg(short, long, value_name = "ADDR:PORT")]
    listen: String,
    /// Capacity of the request queue
    #[arg(short, long, value_name = "CAP_NUM_OF_TASK", default_value_t = 1000000)]
    qcap: u32,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    // Intialize remote scheduler
    let (queue_tx, queue_rx) = crossbeam::channel::bounded(cli.qcap as usize);
    let manager = Arc::new(Mutex::new(ResourceManager::new()));
    let cvar = Arc::new(Condvar::new());

    // Register signal handler
    set_ctrlc_handler(manager.clone());

    // kick off scheduling thread
    let manager_dup = manager.clone();
    let cvar_dup = cvar.clone();
    thread::spawn(move || schedule(queue_rx, manager_dup, cvar_dup));

    let s = RpcServer::new(&cli.listen, manager.clone(), queue_tx, cvar);
    log::debug!("Scheduler starts listening at {:?}", cli.listen);
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
