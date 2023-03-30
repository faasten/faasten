use std::io::{Read, Write};

use lazy_static::lazy_static;
use log::{debug, warn};
use serde::Deserialize;
use sha2::Sha256;

use labeled::buckle;

use super::BackingStore;
use crate::blobstore::Blobstore;

lazy_static! {
    static ref FSTN_IMAGE_BASE: super::path::Path =
        super::path::Path::parse("home:<T,faasten>").unwrap();
    // home:<T,faasten>:fsutil can be invoked by anyone and grants no privilege.
    static ref FSTN_FSUTIL_POLICY: buckle::Buckle =
        buckle::Buckle::parse("T,T").unwrap();
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

    debug!("creating faasten-supplied fsutil blob...");
    {
        let blobname = localfile2blob(&mut blobstore, &config.fsutil);
        let f = crate::fs::Function {
            memory: 128,
            app_image: blobname,
            runtime_image: python_blob,
            kernel: kernel_blob,
        };
        super::utils::create_gate(
            faasten_fs,
            base_dir.clone(),
            "fsutil".to_string(),
            FSTN_FSUTIL_POLICY.clone(),
            f,
        )
        .expect("link fsutil blob");
    }

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

    let blobname = localfile2blob(&mut blobstore, local_path);
    let base_dir = FSTN_IMAGE_BASE.clone();
    let name = "fsutil".to_string();
    super::utils::delete(fs, base_dir.clone(), name.clone()).expect("delete gate");

    let f = crate::fs::Function {
        memory: 128,
        app_image: blobname,
        runtime_image: get_runtime_blob(fs, "python"),
        kernel: get_kernel_blob(fs),
    };
    super::utils::create_gate(fs, base_dir, name, FSTN_FSUTIL_POLICY.clone(), f)
        .expect("create gate");

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

    let blobname = localfile2blob(&mut blobstore, local_path);
    let mut path = FSTN_IMAGE_BASE.clone();
    path.push_dscrp("python".to_string());
    super::utils::update_blob(fs, path, blobname).expect("update blob");

    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}
