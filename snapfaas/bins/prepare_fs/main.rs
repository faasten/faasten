//! Faasten file system preparer
//!
//! The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
//! Kernels and runtime images are stored as blobs.

use clap::{App, Arg};

pub fn main() {
    env_logger::init();
    let matches = App::new("Faasten FS preparer")
        .version("1.0")
        .arg(
            Arg::with_name("config")
                .value_name("YAML")
                .long("config")
                .takes_value(true)
                .required(true)
                .help("Path to the YAML file telling where to look for kernel and runtime image"),
        )
        .get_matches();

    let fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    snapfaas::fs::bootstrap::prepare_fs(&fs, matches.value_of("config").unwrap());
}
