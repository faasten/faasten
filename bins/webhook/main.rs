use clap::{App, Arg};
use snapfaas::server;

mod app;
mod config;

fn main() -> Result<(), std::io::Error> {
    env_logger::init();

    let matches = App::new("webhook server")
        .arg(
            Arg::with_name("snapfaas config")
                .short("c")
                .long("snapfaas_config")
                .takes_value(true)
                .required(true)
                .help("Path to snapfaas config YAML file"),
        )
        .arg(
            Arg::with_name("app config")
                .short("a")
                .long("app_config")
                .takes_value(true)
                .required(true)
                .help("Path to app config YAML file"),
        )
        .arg(
            Arg::with_name("listen")
                .long("listen")
                .short("l")
                .takes_value(true)
                .value_name("ADDR:PORT")
                .required(true)
                .help("Address to listen on"),
        )
        .arg(Arg::with_name("total memory")
                .long("mem")
                .takes_value(true)
                .value_name("MB")
                .required(true)
                .help("Total memory available for all VMs")
        )
        .get_matches();

    let app = app::App::new(matches.value_of("app config").unwrap());
    let server = server::Server::new(
        matches.value_of("total memory").unwrap().parse::<usize>().expect("Total memory is not a valid integer"),
        matches.value_of("snapfaas config").unwrap(),
        matches.value_of("listen").unwrap(),
        app
    );
    server.set_ctrlc_handler();
    server.run()
}
