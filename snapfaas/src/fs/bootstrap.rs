use std::io::{Read, Write};

use lazy_static::lazy_static;
use log::warn;
use serde::Deserialize;
use sha2::Sha256;

use labeled::buckle;

use super::BackingStore;

lazy_static! {
    static ref FAASTEN_HOME: super::path::Path =
        super::path::Path::parse("home:<T,faasten>").unwrap();
    static ref ROOT_PRIV: buckle::Component = buckle::Component::dc_false();
    static ref EMPTY_PRIV: buckle::Component = buckle::Component::dc_true();
}

/// The preparer installs supported kernels and runtime images in the directory `FAASTEN_HOME`.
/// Kernels and runtime images are stored as blobs.
pub fn prepare_fs<S: Clone + BackingStore>(faasten_fs: &super::FS<S>, config_path: &str) {
    #[derive(Deserialize)]
    struct Config {
        kernel: String,
        python: String,
        fsutil: String,
        other_runtimes: Vec<String>,
    }

    let config = std::fs::File::open(config_path).expect("open configuration file");
    let config: Config = serde_yaml::from_reader(config).expect("deserialize");

    let mut blobstore = crate::blobstore::Blobstore::<Sha256>::default();
    let label = buckle::Buckle::parse("T,faasten").unwrap();

    if !faasten_fs.initialize() {
        warn!("Existing root detected. Noop. Exiting.");
        return;
    }

    // bootstrap
    // set up ``home''
    super::utils::set_my_privilge(ROOT_PRIV.clone());
    super::utils::create_faceted(&faasten_fs, super::path::Path::root(), "home".to_string())
        .expect("create ``home'' faceted directory");

    let faasten_principal = vec!["faasten".to_string()];
    let faasten_priv = [buckle::Clause::new_from_vec(vec![faasten_principal])].into();
    super::utils::set_my_privilge(faasten_priv);
    let base_dir = super::path::Path::parse("home:<T,faasten>").unwrap();

    let kernel_blob = {
        let mut kernel = std::fs::File::open(config.kernel).expect("open kernel file");
        let mut blob = blobstore.create().expect("create kernel blob");
        let buf = &mut Vec::new();
        let _ = kernel.read_to_end(buf).expect("read kernel file");
        blob.write_all(buf).expect("write kernel blob");
        let blob = blobstore.save(blob).expect("finalize kernel blob");
        let name = "kernel".to_string();
        super::utils::create_blob(
            &faasten_fs,
            base_dir.clone(),
            name,
            label.clone(),
            blob.name.clone(),
        )
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
        super::utils::create_blob(
            &faasten_fs,
            base_dir.clone(),
            name,
            label.clone(),
            blob.name.clone(),
        )
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
        let f = crate::fs::Function {
            memory: 128,
            app_image: blob.name,
            runtime_image: python_blob.name,
            kernel: kernel_blob.name,
        };
        super::utils::create_gate(
            &faasten_fs,
            base_dir.clone(),
            "fsutil".to_string(),
            label.clone(),
            f,
        )
        .expect("link fsutil blob");
    }

    for rt in config.other_runtimes {
        let mut img = std::fs::File::open(&rt).expect(&format!("open runtime image {:?}", rt));
        let mut blob = blobstore
            .create()
            .expect(&format!("create runtime blob {:?}", rt));
        let buf = &mut Vec::new();
        let _ = img
            .read_to_end(buf)
            .expect(&format!("read runtime file {:?}", rt));
        blob.write_all(buf)
            .expect(&format!("write runtime blob {:?}", rt));
        let blob = blobstore
            .save(blob)
            .expect(&format!("finalize runtime blob {:?}", rt));
        let name = std::path::Path::new(&rt)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        super::utils::create_blob(
            &faasten_fs,
            base_dir.clone(),
            name,
            label.clone(),
            blob.name,
        )
        .expect(&format!("link {:?} blob", rt));
    }
    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}

pub fn register_user_fsutil<S: Clone + BackingStore>(fs: &super::FS<S>, login: String) {
    // generate the per-user fsutil gate, acting on behalf of the user
    let ufacet = buckle::Buckle::parse(&format!("{0},{0}", login)).unwrap();
    super::utils::set_my_privilge(ufacet.integrity.clone());
    let faasten_fsutil = super::path::Path::parse("home:<T,faasten>:fsutil").unwrap();
    let user_home = super::path::Path::parse("~").unwrap();
    if let Err(e) =
        super::utils::dup_gate(fs, faasten_fsutil, user_home, "fsutil".to_string(), ufacet)
    {
        warn!("{:?}", e);
    }
    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}

pub fn get_runtime_blob<S: Clone + BackingStore>(fs: &super::FS<S>, runtime: &str) -> String {
    let mut runtime_fs_path = FAASTEN_HOME.clone();
    runtime_fs_path.push_dscrp(runtime.to_string());
    super::utils::open_blob(fs, runtime_fs_path).unwrap()
}

pub fn get_kernel_blob<S: Clone + BackingStore>(fs: &super::FS<S>) -> String {
    let mut kernel_fs_path = FAASTEN_HOME.clone();
    kernel_fs_path.push_dscrp("kernel".to_string());
    super::utils::open_blob(fs, kernel_fs_path).unwrap()
}
