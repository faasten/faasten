use clap::{App, Arg, crate_authors, crate_version};
use std::sync::{Arc, Mutex};
use std::thread;
use snapfaas::sched::{
    schedule,
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

    // Intialize scheduler
    let sched_addr = matches
                        .value_of("scheduler listen address")
                        .map(String::from)
                        .unwrap();
    let manager = Arc::new(Mutex::new(ResourceManager::new()));
    let manager_dup = Arc::clone(&manager);
    let mut schedgate = SchedGateway::listen(&sched_addr, Some(manager_dup));

    // Start HTTP gateway
    if let Some(l) = matches.value_of("http listen address") {
        let gateway = HTTPGateway::listen(l, None);
        for (request, _timestamps) in gateway {
            // Block until there's resource
            if let Some(_) = schedgate.next() {
                let manager_dup = Arc::clone(&manager);
                thread::spawn(move || {
                    let _ = schedule(request, manager_dup);
                });
            }
        }
    }
}
