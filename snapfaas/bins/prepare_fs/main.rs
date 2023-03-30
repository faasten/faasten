//! Faasten file system preparer
//!
//! The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
//! Kernels and runtime images are stored as blobs.

use clap::{App, ArgGroup};
use snapfaas::blobstore;

pub fn main() {
    env_logger::init();
    let matches = App::new("Faasten FS preparer")
        .args_from_usage(
            "--config        [YAML] 'YAML file specifying where images are at'
             --update_fsutil [PATH] 'PATH to updated fsutil image'
             --update_python [PATH] 'PATH to updated python image'",
        )
        .group(
            ArgGroup::with_name("input")
                .args(&["config", "update_fsutil", "update_python"])
                .required(true),
        )
        .get_matches();

    let fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    let blobstore = blobstore::Blobstore::default();
    if matches.is_present("config") {
        snapfaas::fs::bootstrap::prepare_fs(&fs, matches.value_of("config").unwrap());
    } else if matches.is_present("update_fsutil") {
        snapfaas::fs::bootstrap::update_fsutil(
            &fs,
            blobstore,
            matches.value_of("update_fsutil").unwrap(),
        );
    } else if matches.is_present("update_python") {
        snapfaas::fs::bootstrap::update_python(
            &fs,
            blobstore,
            matches.value_of("update_python").unwrap(),
        );
    } else {
        log::warn!("Noop.");
    }
}
