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
use log::{error, info};
use simple_logger;
use snapfaas::configs;
use snapfaas::controller::Controller;
use snapfaas::gateway;
use snapfaas::gateway::Gateway;
use snapfaas::workerpool;

fn main() {
    simple_logger::init().expect("simple_logger init failed");

    let matches = App::new("SnapFaaS controller")
        .version("1.0")
        .author("David H. Liu <hao.liu@princeton.edu>")
        .about("Launch and configure SnapFaaS controller")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .takes_value(true)
                .help("Controller config YAML file"),
        )
        .arg(
            Arg::with_name("kernel")
                .long("kernel")
                .takes_value(true)
                .help("URL to the kernel binary"),
        )
        .arg(
            Arg::with_name("kernel boot args")
                .long("kernel_args")
                .takes_value(true)
                .default_value("quiet console=none reboot=k panic=1 pci=off")
                .help("Default kernel boot argument"),
        )
        .arg(
            Arg::with_name("requests file")
                .long("requests_file")
                .takes_value(true)
                .help("File containing JSON-lines of requests"),
        )
        .arg(
            Arg::with_name("port number")
                .long("port")
                .short("p")
                .takes_value(true)
                .help("Port on which SnapFaaS accepts requests"),
        )
        .arg(Arg::with_name("total memory")
            .long("mem")
            .takes_value(true)
            .help("Total memory available for all Vms")
        )
        .get_matches();

    // populate the in-memory config struct
    let mut ctr_config = configs::ControllerConfig::new(matches.value_of("config"));

    if let Some(kernel_url) = matches.value_of("kernel") {
        ctr_config.set_kernel_path(kernel_url);
    }

    if let Some(kernel_boot_args) = matches.value_of("kernel boot args") {
        ctr_config.set_kernel_boot_args(kernel_boot_args);
    }

    let mut controller = Controller::new(ctr_config).expect("Cannot create controller");

    if let Some(total_mem) = matches.value_of("total memory") {
        if let Ok(total_mem) = total_mem.parse::<usize>() {
            controller.set_total_mem(total_mem);
        }
    }
    //info!("{:?}", controller);

    // prepare worker pool
    let wp = workerpool::WorkerPool::new(controller);

    // start gateway
    // TODO:support an HTTP gateway in addition to file gateway
    let request_file_url = matches
        .value_of("requests file")
        .expect("Request file not specified");
    let gateway = gateway::FileGateway::listen(request_file_url).expect("Failed to create gateway");

    // start admitting and processing incoming requests
    for task in gateway.incoming() {
        // ignore invalid requests for now
        if task.is_err() {
            error!("Invalid task: {:?}", task);
            continue;
        }

        let (req, rsp_sender) = task.unwrap();
        info!("req (main): {:?}", req);

        wp.send_req(req, rsp_sender);
    }

    wp.shutdown();
}
