use std::io::{Read, Write};

use lazy_static::lazy_static;
use log::{debug, warn};
use serde::Deserialize;
use sha2::Sha256;

use labeled::buckle;

use super::BackingStore;
use crate::blobstore::Blobstore;

const FSUTIL_MEMSIZE: usize = 128;

lazy_static! {
    static ref FSTN_IMAGE_BASE: super::path::Path =
        super::path::Path::parse("home:<T,faasten>").unwrap();
    // home:<T,faasten>:fsutil can be read by anyone but only faasten can update it.
    static ref FSUTIL_POLICY: buckle::Buckle =
        buckle::Buckle::parse("T,faasten").unwrap();
    static ref ROOT_PRIV: buckle::Component = buckle::Component::dc_false();
    static ref EMPTY_PRIV: buckle::Component = buckle::Component::dc_true();
}

fn localfile2blob(blobstore: &mut Blobstore, local_path: &str) -> String {
    let mut f = std::fs::File::open(local_path).expect("open");
    let mut blob = blobstore.create().expect("blobstore create");
    let buf = &mut Vec::new();
    let _ = f.read_to_end(buf).expect("read");
    blob.write_all(buf).expect("write blob");
    let blob = blobstore.save(blob).expect("finalize blob");
    debug!("DONE! local {} to blob {}", local_path, blob.name);
    blob.name
}

fn create_fsutil_redirect_target<S: Clone + BackingStore>(
    faasten_fs: &super::FS<S>,
    blobstore: &mut Blobstore,
    fsutil_local_path: &str,
    python_blob: String,
    kernel_blob: String,
) {
    debug!("creating fsutil redirection target...");
    let blobname = localfile2blob(blobstore, fsutil_local_path);
    super::utils::create_directory(
        faasten_fs,
        FSTN_IMAGE_BASE.clone(),
        "fsutil".to_string(),
        FSUTIL_POLICY.clone(),
    )
    .expect("create redirection target directory");
    let mut target_directory = FSTN_IMAGE_BASE.clone();
    target_directory.push_dscrp("fsutil".to_string());
    super::utils::create_blob(
        faasten_fs,
        target_directory.clone(),
        "app".to_string(),
        FSUTIL_POLICY.clone(),
        blobname,
    )
    .expect("link app image blob");
    super::utils::create_blob(
        faasten_fs,
        target_directory.clone(),
        "runtime".to_string(),
        FSUTIL_POLICY.clone(),
        python_blob,
    )
    .expect("link python image blob");
    super::utils::create_blob(
        faasten_fs,
        target_directory.clone(),
        "kernel".to_string(),
        FSUTIL_POLICY.clone(),
        kernel_blob,
    )
    .expect("link kernel image blob");
    super::utils::create_file(
        faasten_fs,
        target_directory.clone(),
        "memory".to_string(),
        FSUTIL_POLICY.clone(),
    )
    .expect("create memory size file");
    let mut memsize_file = target_directory.clone();
    memsize_file.push_dscrp("memory".to_string());
    super::utils::write(
        faasten_fs,
        memsize_file,
        FSUTIL_MEMSIZE.to_be_bytes().to_vec(),
    )
    .expect("write memory size file");
}

/// The preparer installs supported kernels and runtime images in the directory `FSTN_IMAGE_BASE`.
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

    // start acting as `faasten`
    let faasten_principal = vec!["faasten".to_string()];
    let faasten_priv = [buckle::Clause::new_from_vec(vec![faasten_principal])].into();
    super::utils::set_my_privilge(faasten_priv);
    let base_dir = FSTN_IMAGE_BASE.clone();

    debug!("creating kernel blob...");
    let kernel_blob = {
        let name = "kernel".to_string();
        let blobname = localfile2blob(&mut blobstore, &config.kernel);
        super::utils::create_blob(
            faasten_fs,
            base_dir.clone(),
            name,
            label.clone(),
            blobname.clone(),
        )
        .expect("link kernel blob");
        blobname
    };

    debug!("creating python runtime blob...");
    let python_blob = {
        let blobname = localfile2blob(&mut blobstore, &config.python);
        let name = "python".to_string();
        super::utils::create_blob(
            faasten_fs,
            base_dir.clone(),
            name,
            label.clone(),
            blobname.clone(),
        )
        .expect("link python blob");
        blobname
    };

    create_fsutil_redirect_target(
        faasten_fs,
        &mut blobstore,
        &config.fsutil,
        python_blob,
        kernel_blob,
    );

    for rt in config.other_runtimes {
        debug!("creating {} runtime blob...", rt);
        let blobname = localfile2blob(&mut blobstore, &rt);
        let name = std::path::Path::new(&rt)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        super::utils::create_blob(&faasten_fs, base_dir.clone(), name, label.clone(), blobname)
            .expect(&format!("link {:?} blob", rt));
    }
    super::utils::set_my_privilge(EMPTY_PRIV.clone());
    debug!("Done with bootstrapping.")
}

pub fn register_user_fsutil<S: Clone + BackingStore>(fs: &super::FS<S>, login: String) {
    debug!("Duplicating faasten-supplied fsutil to user-specific fsutil");
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
    let mut runtime_fs_path = FSTN_IMAGE_BASE.clone();
    runtime_fs_path.push_dscrp(runtime.to_string());
    super::utils::open_blob(fs, runtime_fs_path).unwrap()
}

pub fn get_kernel_blob<S: Clone + BackingStore>(fs: &super::FS<S>) -> String {
    let mut kernel_fs_path = FSTN_IMAGE_BASE.clone();
    kernel_fs_path.push_dscrp("kernel".to_string());
    super::utils::open_blob(fs, kernel_fs_path).unwrap()
}

pub fn update_fsutil<S: Clone + BackingStore>(
    fs: &super::FS<S>,
    mut blobstore: Blobstore,
    local_path: &str,
) {
    let faasten_principal = vec!["faasten".to_string()];
    let faasten_priv = [buckle::Clause::new_from_vec(vec![faasten_principal])].into();
    super::utils::set_my_privilge(faasten_priv);

    let mut target_directory = FSTN_IMAGE_BASE.clone();
    target_directory.push_dscrp("fsutil".to_string());

    let blobname = localfile2blob(&mut blobstore, local_path);
    let mut app_path = target_directory.clone();
    app_path.push_dscrp("app".to_string());
    super::utils::update_blob(fs, app_path, blobname).expect("repoint app blob");

    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}

pub fn update_python<S: Clone + BackingStore>(
    fs: &super::FS<S>,
    mut blobstore: Blobstore,
    local_path: &str,
) {
    let faasten_principal = vec!["faasten".to_string()];
    let faasten_priv = [buckle::Clause::new_from_vec(vec![faasten_principal])].into();
    super::utils::set_my_privilge(faasten_priv);

    debug!("repointing :home:<T,faasten>:python...");
    let blobname = localfile2blob(&mut blobstore, local_path);
    let mut path = FSTN_IMAGE_BASE.clone();
    path.push_dscrp("python".to_string());
    super::utils::update_blob(fs, path, blobname.clone()).expect("repoint python blob");

    debug!("repointing :home:<T,faasten>:fsutil:runtime...");
    let mut fsutil_runtime_path = FSTN_IMAGE_BASE.clone();
    fsutil_runtime_path.push_dscrp("fsutil".to_string());
    fsutil_runtime_path.push_dscrp("runtime".to_string());
    super::utils::update_blob(fs, fsutil_runtime_path, blobname)
        .expect("repoint fsutil runtime blob");

    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}
