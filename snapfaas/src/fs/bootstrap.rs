use std::io::{Read, Write};

use lazy_static::lazy_static;
use log::{debug, warn};
use serde::Deserialize;
use sha2::Sha256;

use labeled::buckle::{self, Component, Buckle};

use super::{BackingStore, Blob, FsError};
use crate::{blobstore::Blobstore, fs::{Function, DirectGate, DirEntry, Gate}};

const FSUTIL_MEMSIZE: usize = 128;

lazy_static! {
    static ref FSTN_IMAGE_BASE: super::path::Path =
        super::path::Path::parse("home:<T,faasten>").unwrap();
    // home:<T,faasten>:fsutil can be read by anyone but only faasten can update it.
    static ref FSUTIL_POLICY: buckle::Buckle =
        buckle::Buckle::parse("T,faasten").unwrap();
    pub static ref FAASTEN_PRIV: buckle::Component = {
        let faasten_principal = vec!["faasten".to_string()];
        [buckle::Clause::new_from_vec(vec![faasten_principal])].into()
    };
}

const ROOT_PRIV: buckle::Component = buckle::Component::dc_false();
const EMPTY_PRIV: buckle::Component = buckle::Component::dc_true();

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
pub fn prepare_fs<S: BackingStore>(fs: &super::FS<S>, config_path: &str) -> Result<(), FsError> {
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

    if !fs.initialize() {
        warn!("Existing root detected.");
        //return;
    }

    // bootstrap
    // set up ``home''
    super::utils::set_my_privilge(ROOT_PRIV.clone());
    if super::utils::create_faceted(&fs, super::path::Path::root(), "home".to_string()).is_err() {
        log::warn!("`home` exists");

    }

    // start acting as `faasten`
    super::utils::set_my_privilge(FAASTEN_PRIV.clone());

    debug!("creating kernel blob...");
    let kernel_blob = {
        let name = "kernel".to_string();
        let blobname = localfile2blob(&mut blobstore, &config.kernel);
        super::utils::create_or_update_blob(
            fs,
            FSTN_IMAGE_BASE.clone(),
            name,
            label.clone(),
            blobname.clone(),
        )?;
        blobname
    };

    debug!("creating python runtime blob...");
    let python_blob = {
        let blobname = localfile2blob(&mut blobstore, &config.python);
        let name = "python".to_string();
        super::utils::create_or_update_blob(
            fs,
            FSTN_IMAGE_BASE.clone(),
            name,
            label.clone(),
            blobname.clone(),
        )?;
        blobname
    };

    debug!("creating fsutil blob...");
    let fsutil_blob = {
        let blobname = localfile2blob(&mut blobstore, &config.fsutil);
        let name = "fsutil_image".to_string();
        super::utils::create_or_update_blob(
            fs,
            FSTN_IMAGE_BASE.clone(),
            name,
            label.clone(),
            blobname.clone(),
        )?;
        blobname
    };

    {
        debug!("creating fsutil gate...");
        let function = Function {
            memory: FSUTIL_MEMSIZE,
            app_image: fsutil_blob,
            runtime_image: python_blob,
            kernel: kernel_blob,
        };

        if let DirEntry::Directory(dir) = fs.read_path(FSTN_IMAGE_BASE.clone())? {
            let name: String = "fsutil".into();
            match dir.list(fs).get(&name) {
                Some(DirEntry::Gate(gate)) => {
                    gate.replace(Gate::Direct(DirectGate {
                        privilege: buckle::Component::dc_true(),
                        invoker_integrity_clearance: buckle::Component::dc_true(),
                        declassify: buckle::Component::dc_true(),
                        function,
                    }), fs)?;
                },
                Some(_) => {
                    dir.unlink(&name, fs)?;
                    let gate = fs.create_direct_gate(FSUTIL_POLICY.clone(), DirectGate { privilege: buckle::Component::dc_true(), invoker_integrity_clearance: buckle::Component::dc_true(), declassify: buckle::Component::dc_true(),  function }).expect("create gate");
                    dir.link(name, gate, fs)?;
                },
                None => {
                    let gate = fs.create_direct_gate(FSUTIL_POLICY.clone(), DirectGate { privilege: buckle::Component::dc_true(), invoker_integrity_clearance: buckle::Component::dc_true(), declassify: buckle::Component::dc_true(), function }).expect("create gate");
                    dir.link(name, gate, fs)?;
                }
            }
        } else {
            Err(FsError::BadPath)?
        }
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
        super::utils::create_or_update_blob(
            &fs,
            FSTN_IMAGE_BASE.clone(),
            name,
            label.clone(),
            blobname,
        )
        .expect(&format!("link {:?} blob", rt));
    }
    super::utils::set_my_privilge(EMPTY_PRIV.clone());
    debug!("Done with bootstrapping.");
    Ok(())
}

fn dup_fsutil<S: BackingStore>(
    fs: &super::FS<S>,
    privilege: Component,
    invoker_integrity_clearance: Component,
) -> Result<(), FsError> {
    let faasten_fsutil = super::path::Path::parse("home:<T,faasten>:fsutil").unwrap();
    let base_dir = super::path::Path::parse("~").unwrap();
    match fs.read_path(faasten_fsutil)? {
        super::DirEntry::Gate(gate) => {
            let new_gate = fs.create_redirect_gate(Buckle::public(), super::RedirectGate {
                privilege: privilege.clone(), invoker_integrity_clearance, declassify: privilege, gate
            })?;
            fs.link(base_dir, "fsutil".into(), new_gate)
        },
        _ => Err(super::errors::FsError::NotAGate),
    }
}

pub fn register_user_fsutil<S: BackingStore>(fs: &super::FS<S>, user: Component, clearance: Component) {
    debug!("Duplicating faasten-supplied fsutil to user-specific fsutil");
    // generate the per-user fsutil gate, acting on behalf of the user
    super::utils::set_my_privilge(user.clone());

    match dup_fsutil(fs, user.clone(), clearance.clone()) {
        Err(e) => warn!("{:?}", e),
        _ => (),
    }

    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}

pub fn get_runtime_blob<S: BackingStore>(fs: &super::FS<S>, runtime: &str) -> Blob {
    let mut runtime_fs_path = FSTN_IMAGE_BASE.clone();
    runtime_fs_path.push_dscrp(runtime.to_string());
    fs.open_blob(runtime_fs_path).unwrap()
}

pub fn get_kernel_blob<S: BackingStore>(fs: &super::FS<S>) -> Blob {
    let mut kernel_fs_path = FSTN_IMAGE_BASE.clone();
    kernel_fs_path.push_dscrp("kernel".to_string());
    fs.open_blob(kernel_fs_path).unwrap()
}

pub fn update_fsutil<S: BackingStore>(
    _fs: &super::FS<S>,
    _blobstore: Blobstore,
    _local_path: &str,
) {
    super::utils::set_my_privilge(FAASTEN_PRIV.clone());

    // TODO

    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}

pub fn update_python<S: BackingStore>(
    fs: &super::FS<S>,
    mut blobstore: Blobstore,
    local_path: &str,
) {
    super::utils::set_my_privilge(FAASTEN_PRIV.clone());

    debug!("repointing :home:<T,faasten>:python...");
    let blobname = localfile2blob(&mut blobstore, local_path);
    let mut path = FSTN_IMAGE_BASE.clone();
    path.push_dscrp("python".to_string());
    fs.replace_blob(path, blobname.clone()).expect("repoint python blob");

    super::utils::set_my_privilge(EMPTY_PRIV.clone());
}
