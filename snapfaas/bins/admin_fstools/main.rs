//! Faasten file system preparer
//!
//! The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
//! Kernels and runtime images are stored as blobs.

use clap::{App, ArgGroup};
use snapfaas::blobstore;
use std::io::{Write, stdout};

pub fn main() {
    env_logger::init();
    let matches = App::new("Faasten FS Admin Tools")
        .args_from_usage(
            "--bootstrap     [YAML] 'YAML file for bootstraping an empty Faasten-FS'
             --update_fsutil [PATH] 'PATH to updated fsutil image'
             --update_python [PATH] 'PATH to updated python image'
             --list          [PATH] 'PATH to a directory or a faceted directory, acting as [faasten]'
             --read          [PATH] 'PATH to a file, acting as [faasten]'",
        )
        .group(
            ArgGroup::with_name("input")
                .args(&["bootstrap", "update_fsutil", "update_python", "list", "read"])
                .required(true),
        )
        .get_matches();

    let fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    let blobstore = blobstore::Blobstore::default();
    if matches.is_present("bootstrap") {
        snapfaas::fs::bootstrap::prepare_fs(&fs, matches.value_of("bootstrap").unwrap());
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
    } else if matches.is_present("list") {
        let clearance = labeled::buckle::Buckle::parse("faasten,T").unwrap();
        snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());
        snapfaas::fs::utils::set_clearance(clearance);

        let path = snapfaas::fs::path::Path::parse(matches.value_of("list").unwrap()).unwrap();
        match snapfaas::fs::utils::list(&fs, path) {
            Ok(entries) => {
                for (name, dent) in entries {
                    println!("{}\t{:?}", name, dent);
                }
            }
            Err(e) => log::warn!("Failed list. {:?}", e),
        }
    } else if matches.is_present("read") {
        let clearance = labeled::buckle::Buckle::parse("faasten,T").unwrap();
        snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());
        snapfaas::fs::utils::set_clearance(clearance);

        let path = snapfaas::fs::path::Path::parse(matches.value_of("read").unwrap()).unwrap();
        match snapfaas::fs::utils::read(&fs, path) {
            Ok(mut data) => {
                stdout().write(&mut data).unwrap();
            }
            Err(e) => log::warn!("Failed read. {:?}", e),
        }
    } else {
        log::warn!("Noop.");
    }
}
