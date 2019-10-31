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

use std::string::String;
use log::{error, warn, info};
use url::{Url, ParseError};
use clap::{Arg, App};
use simple_logger;
use shellexpand;

mod configs;

const DEFAULT_CONTROLLER_CONFIG_URL: &str = "file://localhost/etc/snapfaas/default-conf.yaml";
const LOCAL_FILE_URL_PREFIX: &str = "file://localhost";

fn main() {

    simple_logger::init().expect("simple_logger init failed");

    let matches = App::new("SnapFaaS controller")
                          .version("1.0")
                          .author("David H. Liu <hao.liu@princeton.edu>")
                          .about("Launch and configure SnapFaaS controller")
                          .arg(Arg::with_name("config")
                               .short("c")
                               .long("config")
                               .takes_value(true)
                               .help("Controller config YAML file"))
                          .arg(Arg::with_name("kernel")
                               .long("kernel")
                               .takes_value(true)
                               .help("URL to the kernel binary"))
                          .arg(Arg::with_name("kernel boot args")
                               .long("kernel_args")
                               .takes_value(true)
                               .default_value("quiet console=none reboot=k panic=1 pci=off")
                               .help("Default kernel boot argument"))
                          .arg(Arg::with_name("requests file")
                               .long("requests_file")
                               .takes_value(true)
                               .help("File containing JSON-lines of requests"))
                          .get_matches();

    // populate the in-memory config struct
    let ctr_config_url: String = match matches.value_of("config") {
        None => DEFAULT_CONTROLLER_CONFIG_URL.to_string(),
        Some(ctr_config_url) => convert_fs_path_to_url(ctr_config_url),
    };
    info!("Using controller config: {}", ctr_config_url);

    let mut ctr_config = configs::ControllerConfig::new(&ctr_config_url);

    if let Some(kernel_url) = matches.value_of("kernel") {
        ctr_config.kernel_path = convert_fs_path_to_url(kernel_url);
    };

    if let Some(kernel_boot_args) = matches.value_of("kernel boot args") {
        ctr_config.kernel_boot_args = kernel_boot_args.to_string();
    };

    info!("Current config: {:?}", ctr_config);



    // prepare worker pool


    // start request manager


    // start admitting and processing incoming requests

}

/// check if a string is a url string
/// TODO: maybe a more comprehensive check is needed but low priority
fn check_url(path: &str) -> bool {
    return path.starts_with("file://")
         | path.starts_with("http://")
         | path.starts_with("https://")
         | path.starts_with("ftp://");
}


/// Check is a path is local filesystem path. If yes,
/// append file://localhost/ to local filesystem paths and expand ., .. and ~.
/// TODO: maybe a more comprehensive implementation is needed but low priority
fn convert_fs_path_to_url (path: &str) -> String {
    if check_url(path) {
        return path.to_string();
    }
    let mut url = String::from(LOCAL_FILE_URL_PREFIX);
    url.push_str(&shellexpand::tilde(path));

    return url;
}
