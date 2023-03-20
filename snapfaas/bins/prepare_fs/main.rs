//! Faasten file system preparer
//!
//! The preparer installs supported kernels and runtime images in the directory ``home:^T,faasten''.
//! Kernels and runtime images are stored as blobs.

use std::{io::{Read, Write}, path::Path};

use clap::{App, Arg};
use labeled::buckle::{self, Component, Clause};
use log::warn;
use serde::Deserialize;
use sha2::Sha256;

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

    #[derive(Deserialize)]
    struct Config {
        kernel: String,
        python: String,
        fsutil: String,
        other_runtimes: Vec<String>,
    }

    let config = std::fs::File::open(matches.value_of("config").unwrap())
        .expect("open configuration file");
    let config: Config = serde_yaml::from_reader(config).expect("deserialize");

    let faasten_fs = snapfaas::fs::FS::new(&*snapfaas::labeled_fs::DBENV);
    let mut blobstore = snapfaas::blobstore::Blobstore::<Sha256>::default();
    let base_dir = &snapfaas::syscall_server::str_to_syscall_path("home:^T,faasten").unwrap();
    let label = buckle::Buckle::parse("T,faasten").unwrap();

    // bootstrap
    if !faasten_fs.initialize() {
        warn!("Existing root detected. Noop. Exiting.");
    } else {
        // set up ``home''
        let rootpriv = Component::dc_false();
        snapfaas::fs::utils::set_my_privilge(rootpriv);
        snapfaas::fs::utils::create_faceted(&faasten_fs, &Vec::new(), "home".to_string())
            .expect("create ``home'' faceted directory");

        let faasten_principal = vec!["faasten".to_string()];
        snapfaas::fs::utils::set_my_privilge([Clause::new_from_vec(vec![faasten_principal])].into());
        let kernel_blob = {
            let mut kernel = std::fs::File::open(config.kernel).expect("open kernel file");
            let mut blob = blobstore.create().expect("create kernel blob");
            let buf = &mut Vec::new();
            let _ = kernel.read_to_end(buf).expect("read kernel file");
            blob.write_all(buf).expect("write kernel blob");
            let blob = blobstore.save(blob).expect("finalize kernel blob");
            let name = "kernel".to_string();
            snapfaas::fs::utils::create_blob(&faasten_fs, base_dir, name, label.clone(), blob.name.clone())
                .expect("link kernel blob");
            blob
        };

        let python_blob = {
            let mut python = std::fs::File::open(config.python).expect("open python file");
            let mut blob = blobstore.create().expect("create python blob");
            let buf = &mut Vec::new();
            let _ = python.read_to_end(buf).expect("read python file");
            blob.write_all(buf).expect("write python blob");
            let blob = blobstore.save(blob).expect("finalize python blob");
            let name = "python".to_string();
            snapfaas::fs::utils::create_blob(&faasten_fs, base_dir, name, label.clone(), blob.name.clone())
                .expect("link python blob");
            blob
        };

        {
            let mut fsutil = std::fs::File::open(config.fsutil).expect("open fsutil file");
            let mut blob = blobstore.create().expect("create fsutil blob");
            let buf = &mut Vec::new();
            let _ = fsutil.read_to_end(buf).expect("read fsutil file");
            blob.write_all(buf).expect("write fsutil blob");
            let blob = blobstore.save(blob).expect("finalize fsutil blob");
            let f = snapfaas::fs::Function {
                memory: 128,
                app_image: blob.name,
                runtime_image: python_blob.name,
                kernel: kernel_blob.name,
            };
            snapfaas::fs::utils::create_gate(&faasten_fs, base_dir, "fsutil".to_string(), label.clone(), f)
                .expect("link fsutil blob");
        }

        for rt in config.other_runtimes {
            let mut img = std::fs::File::open(&rt).expect(&format!("open runtime image {:?}", rt));
            let mut blob = blobstore.create().expect(&format!("create runtime blob {:?}", rt));
            let buf = &mut Vec::new();
            let _ = img.read_to_end(buf).expect(&format!("read runtime file {:?}", rt));
            blob.write_all(buf).expect(&format!("write runtime blob {:?}", rt));
            let blob = blobstore.save(blob).expect(&format!("finalize runtime blob {:?}", rt));
            let name = Path::new(&rt).file_name().unwrap().to_str().unwrap().to_string();
            snapfaas::fs::utils::create_blob(&faasten_fs, base_dir, name, label.clone(), blob.name)
                .expect(&format!("link {:?} blob", rt));
        }
    }
}
