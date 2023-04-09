use clap::{crate_authors, crate_version, App, Arg};
use std::thread;
use std::time::Duration;
use labeled::buckle::Buckle;
use snapfaas::fs;

const DEFAULT_INTERVAL: &str = "60";

fn main() {
    env_logger::init();

    let matches = App::new("Faasten Garbage Collector")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Launch Garbage Collector")
        .arg(
            Arg::with_name("fs_addr")
                .value_name("[ADDR:]PORT")
                .long("fs_addr")
                .short("a")
                .takes_value(true)
                .required(false)
                .help("Address on which Faasten connects the remote datastore"),
        )
        .arg(
            Arg::with_name("interval")
                .value_name("SECOND(S)")
                .long("interval")
                .short("i")
                .takes_value(true)
                .required(false)
                .help("Time interval between every two garbage collections"),
        )
        .arg(
            Arg::with_name("once")
                .value_name("ONCE")
                .long("once")
                .takes_value(false)
                .required(false)
                .help("Time interval between every two garbage collections"),
        )
        .get_matches();

    let interval = matches
        .value_of("interval")
        .unwrap_or(DEFAULT_INTERVAL)
        .parse::<u64>()
        .expect("interval not int");

    if let Some(_) = matches.value_of("fs_addr").map(String::from) {
        todo!();
    } else {
        fs::utils::taint_with_label(Buckle::top());
        let mut fs = fs::FS::new(&*snapfaas::fs::lmdb::DBENV);
        loop {
            if let Ok(collected) = fs.collect_garbage() {
                log::debug!("garbage collected {}", collected.len())
            }
            if matches.is_present("once") {
                break
            } else {
                thread::sleep(Duration::new(interval, 0));
            }
        }
    }
}
