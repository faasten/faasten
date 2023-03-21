use clap::{App, Arg, crate_authors, crate_version};

use std::sync::{Arc, Mutex};

use snapfaas::sched::{rpc_server::RpcServer, resource_manager::ResourceManager};

fn main() {
    env_logger::init();

    let matches = App::new("Faasten Gateway")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Launch Faasten gateway")
        .arg(
            Arg::with_name("config")
                .value_name("CONFIG YAML")
                .long("prepare_fs")
                .takes_value(true)
                .required(false)
                .help("Path to the YAML file telling where to look for kernel and runtime image"),
        )
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

    snapfaas::prepare_fs(matches.value_of("config").unwrap());

    let sched_addr = matches
                        .value_of("scheduler listen address")
                        .map(String::from)
                        .unwrap();
    let qcap = matches.value_of("queue capacity").unwrap().parse::<usize>().unwrap();
    let manager = Arc::new(Mutex::new(ResourceManager::new()));

    // Register signal handler
    set_ctrlc_handler(manager.clone());

    let s = RpcServer::new(&sched_addr, manager.clone(), qcap);
    s.run();
}

fn set_ctrlc_handler(manager: Arc<Mutex<ResourceManager>>) {
    ctrlc::set_handler(move || {
        log::warn!("{}", "Handling Ctrl-C. Shutting down...");
        manager.lock().unwrap().remove_all();
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
}
