//! Faasten file system preparer
//!
//! The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
//! Kernels and runtime images are stored as blobs.

use clap::{App, ArgGroup, Arg};
use sha2::Sha256;
use snapfaas::blobstore;
use std::io::{stdout, Write};

pub fn main() -> std::io::Result<()> {
    env_logger::init();
    let matches = App::new("Faasten FS Admin Tools")
        .args_from_usage(
            "--bootstrap     [YAML] 'YAML file for bootstraping an empty Faasten-FS'
             --update_fsutil [PATH] 'PATH to updated fsutil image'
             --update_python [PATH] 'PATH to updated python image'
             --list          [PATH] 'PATH to a directory or a faceted directory, acting as [faasten]'
             --faceted-list  [PATH] 'PATH to a directory or a faceted directory, acting as [faasten]'
             --read          [PATH] 'PATH to a file, acting as [faasten]'
             --blob          [PATH] [DEST] [LABEL]
             --mkdir         [PATH] [LABEL]
             --delete        [PATH]",
        )
        .group(
            ArgGroup::with_name("input")
                .args(&["bootstrap", "update_fsutil", "update_python", "faceted-list", "list", "read", "blob", "mkdir", "delete"])
                .required(true),
        )
        .arg(
            Arg::with_name("tikv")
                .long("tikv")
                .value_name("TIKV")
                .takes_value(true)
                .required(false)
                .help("Use TiVK as the backing store.")
        )
        .get_matches();

    let fs = snapfaas::fs::FS::new(
        match matches.value_of("tikv").map(String::from) {
            None => {
                FSWrapper(Box::new(&*snapfaas::fs::lmdb::DBENV))
            }
            Some(tikv_pd) => {
                let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                let client = rt.block_on(async { tikv_client::RawClient::new(vec![tikv_pd], None).await.unwrap() });
                FSWrapper(Box::new(snapfaas::fs::tikv::TikvClient::new(client, std::sync::Arc::new(rt))))
            }
        }
    );

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
    } else if matches.is_present("faceted-list") {
        let clearance = labeled::buckle::Buckle::parse("faasten,T").unwrap();
        snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());
        snapfaas::fs::utils::taint_with_label(clearance.clone());
        snapfaas::fs::utils::set_clearance(clearance);

        let path =
            snapfaas::fs::path::Path::parse(matches.value_of("faceted-list").unwrap()).unwrap();
        match snapfaas::fs::utils::faceted_list(&fs, path) {
            Ok(entries) => {
                for (f, entries) in entries {
                    println!("{}", f);
                    for entry in entries {
                        println!("\t{:?}", entry);
                    }
                }
            }
            Err(e) => log::warn!("Failed list. {:?}", e),
        }
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
    } else if matches.is_present("blob") {
        snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

        let mut args = matches.values_of("blob").unwrap();
        let path = args.next().unwrap();
        let mut file = std::fs::File::open(path)?;
        let dest = snapfaas::fs::path::Path::parse(args.next().unwrap()).unwrap();
        let label = labeled::buckle::Buckle::parse(args.next().unwrap()).unwrap();
        let mut blobstore: blobstore::Blobstore<Sha256> = snapfaas::blobstore::Blobstore::default();
        let mut blob = blobstore.create().unwrap();
        let _ = std::io::copy(&mut file, &mut blob);
        let blob = blobstore.save(blob).unwrap();
        println!(
            "{}",
            snapfaas::fs::utils::create_blob(
                &fs,
                dest.parent().unwrap(),
                dest.file_name().unwrap(),
                label,
                blob.name
            )
            .is_ok()
        );
    } else if matches.is_present("mkdir") {
        snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

        let mut args = matches.values_of("mkdir").unwrap();
        let dest = snapfaas::fs::path::Path::parse(args.next().unwrap()).unwrap();
        let label = labeled::buckle::Buckle::parse(args.next().unwrap()).unwrap();
        println!(
            "{}",
            snapfaas::fs::utils::create_directory(
                &fs,
                dest.parent().unwrap(),
                dest.file_name().unwrap(),
                label
            )
            .is_ok()
        );
    } else if matches.is_present("delete") {
        snapfaas::fs::utils::set_my_privilge(snapfaas::fs::bootstrap::FAASTEN_PRIV.clone());

        let arg = matches.value_of("delete").unwrap();
        let path = snapfaas::fs::path::Path::parse(arg).unwrap();
        println!(
            "{}",
            snapfaas::fs::utils::delete(&fs, path.parent().unwrap(), path.file_name().unwrap())
                .is_ok()
        );
    } else {
        log::warn!("Noop.");
    }
    Ok(())
}

struct FSWrapper(Box<dyn snapfaas::fs::BackingStore>);

impl snapfaas::fs::BackingStore for FSWrapper {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> { self.0.get(key) }
    fn put(&self, key: &[u8], value: &[u8]) { self.0.put(key, value) }
    fn add(&self, key: &[u8], value: &[u8]) -> bool { self.0.add(key, value) }
    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8]) -> Result<(), Option<Vec<u8>>> {
        self.0.cas(key, expected, value)
    }
    fn del(&self, key: &[u8]) { self.0.del(key) }
    fn get_keys(&self) -> Option<Vec<&[u8]>> { self.0.get_keys() }
}
