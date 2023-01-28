use clap::{App, Arg, crate_authors, crate_version};
use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::thread;
use snapfaas::sched::{
    schedule_sync,
    rpc::Scheduler,
    gateway::{
        Gateway,
        HTTPGateway,
        SchedGateway,
    },
    resource_manager::ResourceManager,
};

fn main() {
    env_logger::init();

    let matches = App::new("Faasten Gateway")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Launch Faasten gateway")
        .arg(
            Arg::with_name("http listen address")
                .value_name("[ADDR:]PORT")
                .long("listen_http")
                .short("l")
                .takes_value(true)
                .required(true)
                .help("Address on which Faasten listen for connections that sends requests"),
        )
        .arg(
            Arg::with_name("scheduler listen address")
                .value_name("[ADDR:]PORT")
                .long("listen_sched")
                .short("s")
                .takes_value(true)
                .required(true)
                .help("Address on which Faasten listen for RPCs that requests for tasks"),
        )
        .get_matches();

    // Intialize remote scheduler
    let sched_addr = matches
                        .value_of("scheduler listen address")
                        .map(String::from)
                        .unwrap();
    let manager = Arc::new(Mutex::new(ResourceManager::new()));
    let mut sched_gateway = SchedGateway::listen(&sched_addr, Some(Arc::clone(&manager)));
    let _ = sched_addr.parse::<SocketAddr>().expect("invalid socket address");

    // Register signal handler
    set_ctrlc_handler(sched_addr.clone());

    // TCP gateway
    if let Some(l) = matches.value_of("http listen address") {
        let gateway = HTTPGateway::listen(l, None);
        for (request, tx) in gateway {
            // Block until a worker is available
            if let Some(_) = sched_gateway.next() {
                let sched_resman_dup = Arc::clone(&manager);
                thread::spawn(move || {
                    let _ = schedule_sync(request, sched_resman_dup, tx).unwrap();
                });
            }
        }
    }
}

fn set_ctrlc_handler(sched_addr: String) {
    ctrlc::set_handler(move || {
        log::warn!("{}", "Handling Ctrl-C. Shutting down...");
        let mut sched = Scheduler::new(sched_addr.clone());
        let _ = sched.terminate_all();
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
}
