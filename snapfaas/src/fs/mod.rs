pub mod bootstrap;
///! Labeled File System
pub mod path;

use labeled::{
    buckle::{Buckle, Component, Principal},
    Label,
};
use lmdb::{Cursor, Transaction, WriteFlags};
use log::debug;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use crate::configs::FunctionConfig;

pub use errors::*;

use self::utils::check_delegation;

thread_local!(pub static CURRENT_LABEL: RefCell<Buckle> = RefCell::new(Buckle::public()));
thread_local!(pub static PRIVILEGE: RefCell<Component> = RefCell::new(Component::dc_true()));
thread_local!(pub static CLEARANCE: RefCell<Buckle> = RefCell::new(Buckle::top()));
thread_local!(static STAT: RefCell<Metrics> = RefCell::new(Metrics::default()));

type UID = u64;

#[derive(Default, Clone, Debug, Serialize)]
pub struct Metrics {
    get: Duration,
    get_key_bytes: usize,
    get_val_bytes: usize,
    put: Duration,
    put_key_bytes: usize,
    put_val_bytes: usize,
    add: Duration,
    add_key_bytes: usize,
    add_val_bytes: usize,
    cas: Duration,
    cas_key_bytes: usize,
    cas_val_bytes: usize,
    ser_dir: Duration,
    ser_faceted: Duration,
    ser_label: Duration,
    de_dir: Duration,
    de_faceted: Duration,
    create_retry: i64,
    label_tracking: Duration,
}

pub mod metrics {
    use super::*;

    pub fn get_stat() -> Metrics {
        STAT.with(|stat| stat.borrow().clone())
    }
}

pub trait BackingStore {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn put(&self, key: &[u8], value: &[u8]);
    fn add(&self, key: &[u8], value: &[u8]) -> bool;
    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8])
        -> Result<(), Option<Vec<u8>>>;
    fn del(&self, key: &[u8]) -> bool;
    fn get_keys(&self) -> Option<Vec<&[u8]>>;
}

impl BackingStore for &lmdb::Environment {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        STAT.with(|stat| {
            let now = Instant::now();
            let db = self.open_db(None).ok()?;
            let txn = self.begin_ro_txn().ok()?;
            let res = txn.get(db, &key).ok().map(Into::<Vec<u8>>::into);
            txn.commit().ok()?;
            stat.borrow_mut().get += now.elapsed();
            stat.borrow_mut().get_val_bytes += res.as_ref().map_or(0, |v| v.len());
            stat.borrow_mut().get_key_bytes += key.len();
            res
        })
    }

    fn put(&self, key: &[u8], value: &[u8]) {
        STAT.with(|stat| {
            let now = Instant::now();
            let db = self.open_db(None).unwrap();
            let mut txn = self.begin_rw_txn().unwrap();
            let _ = txn.put(db, &key, &value, WriteFlags::empty());
            txn.commit().unwrap();
            stat.borrow_mut().put += now.elapsed();
            stat.borrow_mut().put_val_bytes += value.len();
            stat.borrow_mut().put_key_bytes += key.len();
        })
    }

    fn add(&self, key: &[u8], value: &[u8]) -> bool {
        STAT.with(|stat| {
            let now = Instant::now();
            let db = self.open_db(None).unwrap();
            let mut txn = self.begin_rw_txn().unwrap();
            let res = match txn.put(db, &key, &value, WriteFlags::NO_OVERWRITE) {
                Ok(_) => true,
                Err(_) => false,
            };
            txn.commit().unwrap();
            stat.borrow_mut().add += now.elapsed();
            stat.borrow_mut().add_val_bytes += value.len();
            stat.borrow_mut().add_key_bytes += key.len();
            res
        })
    }

    fn cas(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        value: &[u8],
    ) -> Result<(), Option<Vec<u8>>> {
        STAT.with(|stat| {
            let now = Instant::now();
            let db = self.open_db(None).unwrap();
            let mut txn = self.begin_rw_txn().unwrap();
            let old = txn.get(db, &key).ok().map(Into::into);
            let res = if expected.map(|e| Vec::from(e)) == old {
                let _ = txn.put(db, &key, &value, WriteFlags::empty());
                Ok(())
            } else {
                Err(old)
            };
            txn.commit().unwrap();
            stat.borrow_mut().cas += now.elapsed();
            if res.is_ok() {
                stat.borrow_mut().cas_val_bytes += value.len();
            }
            stat.borrow_mut().cas_key_bytes += key.len();
            res
        })
    }

    fn del(&self, key: &[u8]) -> bool {
        STAT.with(|_stat| {
            let db = self.open_db(None).unwrap();
            let mut txn = self.begin_rw_txn().unwrap();
            let res = txn.del(db, &key, None).is_ok();
            txn.commit().unwrap();
            res
        })
    }

    fn get_keys(&self) -> Option<Vec<&[u8]>> {
        STAT.with(|_stat| {
            let db = self.open_db(None).ok()?;
            let txn = self.begin_ro_txn().ok()?;
            let mut cursor = txn.open_ro_cursor(db).ok()?;
            let mut keys = Vec::new();
            for data in cursor.iter_start() {
                if let Ok((key, _)) = data {
                    keys.push(key);
                }
            }
            Some(keys)
        })
    }
}

#[derive(Debug)]
pub struct FS<S> {
    storage: S,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directory {
    label: Buckle,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    label: Buckle,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetedDirectory {
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    label: Buckle,
    object_id: UID,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FacetedDirectoryInner {
    facets: Vec<Directory>,
    // allocated lookup
    allocated: HashMap<String, usize>,
    // indexing for single principals, they own categories they compose
    #[serde_as(as = "HashMap<serde_with::json::JsonString, _>")]
    principal_indexing: HashMap<Vec<Principal>, Vec<usize>>,
    // secrecy=dc_true
    public_secrecies: Vec<usize>,
}

impl FacetedDirectoryInner {
    pub fn open_facet(&self, facet: &Buckle) -> Result<Directory, FacetError> {
        STAT.with(|stat| {
            let now = Instant::now();
            let jsonfacet = serde_json::to_string(facet).unwrap();
            stat.borrow_mut().ser_label += now.elapsed();
            CURRENT_LABEL.with(|current_label| {
                if facet.can_flow_to(&*current_label.borrow()) {
                    Ok(self
                        .allocated
                        .get(&jsonfacet)
                        .map(|idx| -> Directory { self.facets.get(idx.clone()).unwrap().clone() })
                        .ok_or(FacetError::Unallocated))
                } else {
                    Err(FacetError::LabelError(LabelError::CannotRead))
                }
            })?
        })
    }

    pub fn dummy_list_facets(&self) -> Vec<Directory> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                self.facets
                    .iter()
                    .filter(|d| {
                        let now = Instant::now();
                        let res = current_label.borrow().secrecy.implies(&d.label.secrecy);
                        stat.borrow_mut().label_tracking += now.elapsed();
                        res
                    })
                    .cloned()
                    .collect()
            })
        })
    }

    pub fn list_facets(&self) -> Vec<Directory> {
        CURRENT_LABEL.with(|current_label| {
            let secrecy = &current_label.borrow().secrecy;
            match secrecy {
                Component::DCFormula(clauses) => {
                    if clauses.len() == 1 {
                        let clause = clauses.iter().next().unwrap();
                        if clause.0.len() == 1 {
                            let p = clause.0.iter().next().unwrap();
                            let mut res = self
                                .principal_indexing
                                .get(p)
                                .map(|v| {
                                    v.iter()
                                        .map(|idx| self.facets[idx.clone()].clone())
                                        .collect::<Vec<Directory>>()
                                })
                                .unwrap_or_default();
                            res.extend(
                                self.public_secrecies
                                    .iter()
                                    .map(|idx| self.facets[idx.clone()].clone()),
                            );
                            res
                        } else {
                            self.dummy_list_facets()
                        }
                    } else if clauses.len() == 0 {
                        self.public_secrecies
                            .iter()
                            .map(|idx| self.facets[idx.clone()].clone())
                            .collect::<Vec<Directory>>()
                    } else {
                        self.dummy_list_facets()
                    }
                }
                Component::DCFalse => self.dummy_list_facets(),
            }
        })
    }

    pub fn append(&mut self, dir: Directory) -> Option<Directory> {
        STAT.with(|stat| {
            let now = Instant::now();
            let facet = serde_json::ser::to_string(&dir.label).unwrap();
            stat.borrow_mut().ser_label += now.elapsed();
            match self.allocated.get(&facet) {
                Some(idx) => Some(self.facets[idx.clone()].clone()),
                None => {
                    self.facets.push(dir.clone());
                    let idx = self.facets.len() - 1;
                    self.allocated.insert(facet, idx);
                    // update principal_indexing
                    match dir.label.secrecy {
                        Component::DCFormula(clauses) => {
                            if clauses.len() > 0 {
                                // find the principal that appears in every clause of the CNF
                                // a literal of the CNF implies the CNF if and only if the literal appears in
                                // every clause.
                                let first = &clauses.iter().next().unwrap().0;
                                let intersected = clauses.iter().fold(first.clone(), |res, c| {
                                    c.0.intersection(&res).cloned().collect()
                                });
                                // for each such principal, update indexing for all its prefixes
                                // including itself
                                for p in intersected.iter() {
                                    for i in 0..=p.len() {
                                        let prefix = p[..i].to_vec();
                                        if !self.principal_indexing.contains_key(&prefix) {
                                            self.principal_indexing
                                                .insert(prefix.clone(), Vec::new());
                                        }
                                        self.principal_indexing.get_mut(&prefix).unwrap().push(idx);
                                    }
                                }
                            } else {
                                // secrecy == dc_true
                                self.public_secrecies.push(idx);
                            }
                        }
                        Component::DCFalse => (),
                    };
                    None
                }
            }
        })
    }
}

#[derive(Default, Clone, Debug, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub struct Function {
    // TODO support snapshots
    pub memory: usize,
    pub app_image: String,
    pub runtime_image: String,
    pub kernel: String,
}

// used by singlevm. singlevm allows more complicated configurations than multivm.
impl From<FunctionConfig> for Function {
    fn from(cfg: FunctionConfig) -> Self {
        Self {
            memory: cfg.memory,
            app_image: cfg.appfs.unwrap_or_default(),
            runtime_image: cfg.runtimefs,
            kernel: cfg.kernel,
        }
    }
}

impl From<crate::syscalls::Function> for Function {
    fn from(pbf: crate::syscalls::Function) -> Self {
        Self {
            memory: pbf.memory as usize,
            app_image: pbf.app_image,
            runtime_image: pbf.runtime,
            kernel: pbf.kernel,
        }
    }
}

impl From<Function> for crate::syscalls::Function {
    fn from(f: Function) -> Self {
        Self {
            memory: f.memory as u64,
            app_image: f.app_image,
            runtime: f.runtime_image,
            kernel: f.kernel,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub privilege: Component,
    weakest_privilege_required: Component,
    redirect: bool,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DirEntry {
    Directory(Directory),
    File(File),
    FacetedDirectory(FacetedDirectory),
    Gate(Gate),
    Blob(Blob),
}

mod errors {
    #[derive(Debug)]
    pub enum LinkError {
        LabelError(LabelError),
        Exists,
    }

    #[derive(Debug)]
    pub enum UnlinkError {
        LabelError(LabelError),
        DoesNotExists,
    }

    #[derive(Debug)]
    pub enum LabelError {
        CannotRead,
        CannotWrite,
    }

    #[derive(Debug)]
    pub enum GateError {
        CannotDelegate,
        CannotInvoke,
        Corrupted,
    }

    #[derive(Debug)]
    pub enum FacetError {
        Unallocated,
        LabelError(LabelError),
        NoneValue,
        Corrupted,
    }
}

impl<S> FS<S> {
    pub fn new(storage: S) -> FS<S> {
        FS { storage }
    }
}

impl<S: BackingStore> FS<S> {
    /// true, the root is newly created; false, the root already exists
    pub fn initialize(&self) -> bool {
        let dir_contents = serde_json::ser::to_vec(&HashMap::<String, DirEntry>::new()).unwrap();
        let uid: UID = 0;
        self.storage.add(&uid.to_be_bytes(), &dir_contents)
    }

    pub fn root(&self) -> Directory {
        Directory {
            label: Buckle::new(true, Component::dc_false()),
            object_id: 0,
        }
    }

    pub fn create_directory(&self, label: Buckle) -> Directory {
        STAT.with(|stat| {
            let now = Instant::now();
            let dir_contents = serde_json::ser::to_vec(&HashMap::<String, DirEntry>::new())
                .unwrap_or((&b"{}"[..]).into());
            stat.borrow_mut().ser_dir += now.elapsed();
            let mut uid: UID = rand::random();
            while !self.storage.add(&uid.to_be_bytes(), &dir_contents) {
                uid = rand::random();
            }

            Directory {
                label,
                object_id: uid,
            }
        })
    }

    pub fn create_file(&self, label: Buckle) -> File {
        STAT.with(|stat| {
            let mut uid: UID = rand::random();
            while !self.storage.add(&uid.to_be_bytes(), &[]) {
                uid = rand::random();
                stat.borrow_mut().create_retry += 1;
            }
            File {
                label,
                object_id: uid,
            }
        })
    }

    pub fn create_faceted_directory(&self) -> FacetedDirectory {
        STAT.with(|stat| {
            let mut uid: UID = rand::random();
            let now = Instant::now();
            let empty_faceted_dir =
                serde_json::ser::to_vec(&FacetedDirectoryInner::default()).unwrap();
            stat.borrow_mut().ser_faceted += now.elapsed();
            while !self.storage.add(&uid.to_be_bytes(), &empty_faceted_dir) {
                uid = rand::random()
            }
            FacetedDirectory { object_id: uid }
        })
    }

    pub fn create_blob(&self, blobname: String, label: Buckle) -> Blob {
        STAT.with(|stat| {
            let mut uid: UID = rand::random();
            while !self
                .storage
                .add(&uid.to_be_bytes(), &blobname.clone().into_bytes())
            {
                uid = rand::random();
                stat.borrow_mut().create_retry += 1;
            }
            Blob {
                label,
                object_id: uid,
            }
        })
    }

    pub fn create_redirect_gate(
        &self,
        dpriv: Component,
        wpr: Component,
        redirect_path: self::path::Path,
    ) -> Result<Gate, GateError> {
        STAT.with(|stat| {
            if check_delegation(&dpriv) {
                let mut uid: UID = rand::random();
                while !self.storage.add(
                    &uid.to_be_bytes(),
                    &serde_json::to_vec(&redirect_path).unwrap_or_default(),
                ) {
                    uid = rand::random();
                    stat.borrow_mut().create_retry += 1;
                }
                Ok(Gate {
                    privilege: dpriv,
                    weakest_privilege_required: wpr,
                    redirect: true,
                    object_id: uid,
                })
            } else {
                Err(GateError::CannotDelegate)
            }
        })
    }

    pub fn create_gate(
        &self,
        dpriv: Component,
        wpr: Component,
        f: Function,
    ) -> Result<Gate, GateError> {
        STAT.with(|stat| {
            if check_delegation(&dpriv) {
                let mut uid: UID = rand::random();
                while !self.storage.add(
                    &uid.to_be_bytes(),
                    &serde_json::to_vec(&f).unwrap_or_default(),
                ) {
                    uid = rand::random();
                    stat.borrow_mut().create_retry += 1;
                }
                Ok(Gate {
                    privilege: dpriv,
                    weakest_privilege_required: wpr,
                    redirect: false,
                    object_id: uid,
                })
            } else {
                Err(GateError::CannotDelegate)
            }
        })
    }

    pub fn list(&self, dir: Directory) -> Result<HashMap<String, DirEntry>, LabelError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                let now = Instant::now();
                if dir.label.can_flow_to(&*current_label.borrow()) {
                    stat.borrow_mut().label_tracking += now.elapsed();
                    Ok(match self.storage.get(&dir.object_id.to_be_bytes()) {
                        Some(bs) => {
                            let now = Instant::now();
                            let res = serde_json::from_slice(bs.as_slice()).unwrap();
                            stat.borrow_mut().de_dir += now.elapsed();
                            res
                        }
                        None => Default::default(),
                    })
                } else {
                    Err(LabelError::CannotRead)
                }
            })
        })
    }

    pub fn faceted_list(
        &self,
        fdir: &FacetedDirectory,
    ) -> HashMap<String, HashMap<String, DirEntry>> {
        STAT.with(
            |stat| match self.storage.get(&fdir.object_id.to_be_bytes()) {
                Some(bs) => {
                    let now = Instant::now();
                    serde_json::from_slice::<FacetedDirectoryInner>(bs.as_slice())
                        .map(|inner| {
                            stat.borrow_mut().de_faceted += now.elapsed();
                            inner.list_facets()
                        })
                        .unwrap_or_default()
                        .iter()
                        .fold(
                            HashMap::<String, HashMap<String, DirEntry>>::new(),
                            |mut m, dir| {
                                let now = Instant::now();
                                m.insert(
                                    serde_json::ser::to_string(dir.label()).unwrap(),
                                    self.list(dir.clone()).unwrap(),
                                );
                                stat.borrow_mut().ser_label += now.elapsed();
                                m
                            },
                        )
                }
                None => Default::default(),
            },
        )
    }

    fn open_facet(&self, fdir: &FacetedDirectory, facet: &Buckle) -> Result<Directory, FacetError> {
        STAT.with(
            |stat| match self.storage.get(&fdir.object_id.to_be_bytes()) {
                Some(bs) => {
                    let now = Instant::now();
                    let inner: FacetedDirectoryInner =
                        serde_json::from_slice(bs.as_slice()).map_err(|_| FacetError::Corrupted)?;
                    stat.borrow_mut().de_faceted += now.elapsed();
                    inner.open_facet(facet)
                }
                None => Err(FacetError::NoneValue),
            },
        )
    }

    pub fn link(
        &self,
        dir: &Directory,
        name: String,
        direntry: DirEntry,
    ) -> Result<String, LinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                let now = Instant::now();
                if !current_label.borrow().secrecy.implies(&dir.label.secrecy) {
                    return Err(LinkError::LabelError(LabelError::CannotRead));
                }
                if !current_label.borrow().can_flow_to(&dir.label) {
                    return Err(LinkError::LabelError(LabelError::CannotWrite));
                }
                stat.borrow_mut().label_tracking += now.elapsed();
                let mut raw_dir: Option<Vec<u8>> = self.storage.get(&dir.object_id.to_be_bytes());
                loop {
                    let mut dir_contents: HashMap<String, DirEntry> = raw_dir
                        .as_ref()
                        .and_then(|dir_contents| {
                            let now = Instant::now();
                            let res = serde_json::from_slice(dir_contents.as_slice()).ok();
                            stat.borrow_mut().de_dir += now.elapsed();
                            res
                        })
                        .unwrap_or_default();
                    if let Some(_) = dir_contents.insert(name.clone(), direntry.clone()) {
                        return Err(LinkError::Exists);
                    }
                    let now = Instant::now();
                    let json_vec = serde_json::to_vec(&dir_contents).unwrap_or_default();
                    stat.borrow_mut().ser_dir += now.elapsed();
                    match self.storage.cas(
                        &dir.object_id.to_be_bytes(),
                        raw_dir.as_ref().map(|e| e.as_ref()),
                        &json_vec,
                    ) {
                        Ok(()) => return Ok(name),
                        Err(rd) => raw_dir = rd,
                    }
                }
            })
        })
    }

    pub fn unlink(&self, dir: &Directory, name: String) -> Result<String, UnlinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                let now = Instant::now();
                if !current_label.borrow().secrecy.implies(&dir.label.secrecy) {
                    return Err(UnlinkError::LabelError(LabelError::CannotRead));
                }
                if !current_label.borrow().can_flow_to(&dir.label) {
                    return Err(UnlinkError::LabelError(LabelError::CannotWrite));
                }
                stat.borrow_mut().label_tracking += now.elapsed();
                let mut raw_dir = self.storage.get(&dir.object_id.to_be_bytes());
                loop {
                    let mut dir_contents: HashMap<String, DirEntry> = raw_dir
                        .as_ref()
                        .and_then(|dir_contents| {
                            let now = Instant::now();
                            let res = serde_json::from_slice(dir_contents.as_slice()).ok();
                            stat.borrow_mut().de_dir += now.elapsed();
                            res
                        })
                        .unwrap_or_default();
                    if dir_contents.remove(&name).is_none() {
                        return Err(UnlinkError::DoesNotExists);
                    }
                    let now = Instant::now();
                    let json_vec = serde_json::to_vec(&dir_contents).unwrap_or_default();
                    stat.borrow_mut().ser_dir += now.elapsed();
                    match self.storage.cas(
                        &dir.object_id.to_be_bytes(),
                        raw_dir.as_ref().map(|e| e.as_ref()),
                        &json_vec,
                    ) {
                        Ok(()) => return Ok(name),
                        Err(rd) => raw_dir = rd,
                    }
                }
            })
        })
    }

    pub fn faceted_link(
        &self,
        fdir: &FacetedDirectory,
        facet: Option<&Buckle>,
        name: String,
        direntry: DirEntry,
    ) -> Result<String, LinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                // check when facet is specified.
                let now = Instant::now();
                if facet.is_some()
                    && !current_label
                        .borrow()
                        .secrecy
                        .implies(&facet.as_ref().unwrap().secrecy)
                {
                    return Err(LinkError::LabelError(LabelError::CannotRead));
                }
                if facet.is_some() && !current_label.borrow().can_flow_to(&facet.as_ref().unwrap())
                {
                    return Err(LinkError::LabelError(LabelError::CannotWrite));
                }
                stat.borrow_mut().label_tracking += now.elapsed();
                let mut raw_fdir: Option<Vec<u8>> = self.storage.get(&fdir.object_id.to_be_bytes());
                loop {
                    let mut fdir_contents: FacetedDirectoryInner = raw_fdir
                        .as_ref()
                        .and_then(|fdir_contents| {
                            let now = Instant::now();
                            let res = serde_json::from_slice(fdir_contents.as_slice()).ok();
                            stat.borrow_mut().de_faceted += now.elapsed();
                            res
                        })
                        .unwrap_or_default();
                    match fdir_contents.open_facet(facet.unwrap_or(&*current_label.borrow())) {
                        Ok(dir) => return Ok(self.link(&dir, name.clone(), direntry.clone())?),
                        Err(FacetError::Unallocated) => {
                            let dir = self.create_directory(current_label.borrow().clone());
                            let _ = self.link(&dir, name.clone(), direntry.clone());
                            fdir_contents.append(dir);
                            let now = Instant::now();
                            let json_vec = serde_json::to_vec(&fdir_contents).unwrap_or_default();
                            stat.borrow_mut().ser_faceted += now.elapsed();
                            match self.storage.cas(
                                &fdir.object_id.to_be_bytes(),
                                raw_fdir.as_ref().map(|e| e.as_ref()),
                                &json_vec,
                            ) {
                                Ok(()) => return Ok(name),
                                Err(rd) => raw_fdir = rd,
                            }
                        }
                        Err(FacetError::LabelError(le)) => return Err(LinkError::LabelError(le)),
                        Err(e) => panic!("fatal: {:?}", e),
                    }
                }
            })
        })
    }

    pub fn faceted_unlink(
        &self,
        fdir: &FacetedDirectory,
        name: String,
    ) -> Result<String, UnlinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                let facet = &*current_label.borrow();
                let raw_fdir = self.storage.get(&fdir.object_id.to_be_bytes());
                let fdir_contents: FacetedDirectoryInner = raw_fdir
                    .as_ref()
                    .and_then(|fdir_contents| {
                        let now = Instant::now();
                        let res = serde_json::from_slice(fdir_contents.as_slice()).ok();
                        stat.borrow_mut().de_faceted += now.elapsed();
                        res
                    })
                    .unwrap_or_default();
                match fdir_contents.open_facet(facet) {
                    Ok(dir) => return Ok(self.unlink(&dir, name.clone())?),
                    Err(FacetError::Unallocated) => return Err(UnlinkError::DoesNotExists),
                    Err(FacetError::LabelError(le)) => return Err(UnlinkError::LabelError(le)),
                    Err(e) => panic!("fatal: {:?}", e),
                }
            })
        })
    }

    pub fn read(&self, file: &File) -> Result<Vec<u8>, LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if file.label.can_flow_to(&*current_label.borrow()) {
                Ok(self
                    .storage
                    .get(&file.object_id.to_be_bytes())
                    .unwrap_or_default())
            } else {
                Err(LabelError::CannotRead)
            }
        })
    }

    pub fn open_blob(&self, blob: &Blob) -> Result<String, LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if blob.label.can_flow_to(&*current_label.borrow()) {
                let v = self
                    .storage
                    .get(&blob.object_id.to_be_bytes())
                    .unwrap_or_default();
                Ok(String::from_utf8(v).unwrap_or_default())
            } else {
                Err(LabelError::CannotRead)
            }
        })
    }

    pub fn update_blob(&self, blob: &Blob, blobname: String) -> Result<(), LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if current_label.borrow().can_flow_to(&blob.label) {
                Ok(self
                    .storage
                    .put(&blob.object_id.to_be_bytes(), &blobname.into_bytes()))
            } else {
                Err(LabelError::CannotWrite)
            }
        })
    }

    pub fn write(&self, file: &File, data: &Vec<u8>) -> Result<(), LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if current_label.borrow().can_flow_to(&file.label) {
                Ok(self.storage.put(&file.object_id.to_be_bytes(), data))
            } else {
                Err(LabelError::CannotWrite)
            }
        })
    }

    pub fn invoke(&self, gate: &Gate) -> Result<Function, GateError> {
        CURRENT_LABEL.with(|current_label| {
            if current_label
                .borrow()
                .integrity
                .implies(&gate.weakest_privilege_required)
            {
                let raw_gate = self
                    .storage
                    .get(&gate.object_id.to_be_bytes())
                    .ok_or(GateError::Corrupted)?;
                Ok(serde_json::from_slice(&raw_gate).map_err(|_| GateError::Corrupted)?)
            } else {
                Err(GateError::CannotInvoke)
            }
        })
    }

    pub fn invoke_redirect(&self, gate: &Gate) -> Result<self::path::Path, GateError> {
        CURRENT_LABEL.with(|current_label| {
            if current_label
                .borrow()
                .integrity
                .implies(&gate.weakest_privilege_required)
            {
                let raw_gate = self
                    .storage
                    .get(&gate.object_id.to_be_bytes())
                    .ok_or(GateError::Corrupted)?;
                Ok(serde_json::from_slice(&raw_gate).map_err(|_| GateError::Corrupted)?)
            } else {
                Err(GateError::CannotInvoke)
            }
        })
    }

    fn faceted_inner(&self, fdir: &FacetedDirectory) -> HashMap<String, DirEntry> {
        match self.storage.get(&fdir.object_id.to_be_bytes()) {
            Some(bs) => serde_json::from_slice::<FacetedDirectoryInner>(bs.as_slice())
                .map(|inner| inner.list_facets())
                .unwrap_or_default()
                .iter()
                .fold(HashMap::<String, DirEntry>::new(), |mut m, dir| {
                    m.insert(
                        serde_json::ser::to_string(dir.label()).unwrap(),
                        DirEntry::Directory(dir.clone()),
                    );
                    m
                }),
            None => Default::default(),
        }
    }

    pub fn collect_garbage(&mut self) -> Result<Vec<u64>, LabelError> {
        use std::convert::TryInto;
        let object_list = self
            .storage
            .get_keys()
            .unwrap_or_default()
            .iter()
            .filter_map(|&b| b.try_into().ok())
            .map(|b| UID::from_be_bytes(b))
            .collect::<Vec<_>>();
        let mut objects = HashSet::new();
        for obj in object_list.into_iter() {
            objects.insert(obj);
        }

        let mut visited = HashSet::new();
        let mut remaining = vec![DirEntry::Directory(self.root())];
        while let Some(entry) = remaining.pop() {
            match entry {
                DirEntry::Directory(dir) => {
                    if !visited.insert(dir.object_id) {
                        continue;
                    }
                    let entries = self.list(dir)?;
                    for entry in entries.into_values() {
                        remaining.push(entry);
                    }
                }
                DirEntry::FacetedDirectory(fdir) => {
                    if !visited.insert(fdir.object_id) {
                        continue;
                    }
                    let faceted_entries = self.faceted_inner(&fdir);
                    for entry in faceted_entries.into_values() {
                        remaining.push(entry);
                    }
                }
                DirEntry::File(file) => {
                    let _ = visited.insert(file.object_id);
                }
                DirEntry::Gate(gate) => {
                    let _ = visited.insert(gate.object_id);
                }
                DirEntry::Blob(blob) => {
                    let _ = visited.insert(blob.object_id);
                }
            }
        }

        assert!(visited.iter().map(|o| objects.contains(o)).all(|x| x));

        let diff = objects.difference(&visited).map(|&x| x).collect::<Vec<_>>();
        for obj in diff.iter() {
            self.storage.del(&obj.to_be_bytes());
        }

        Ok(diff)
    }
}

impl Directory {
    pub fn label(&self) -> &Buckle {
        &self.label
    }
}

impl File {
    pub fn label(&self) -> &Buckle {
        &self.label
    }
}

impl From<Directory> for DirEntry {
    fn from(dir: Directory) -> Self {
        DirEntry::Directory(dir)
    }
}

impl From<File> for DirEntry {
    fn from(file: File) -> Self {
        DirEntry::File(file)
    }
}

impl From<FacetedDirectory> for DirEntry {
    fn from(fdir: FacetedDirectory) -> Self {
        DirEntry::FacetedDirectory(fdir)
    }
}

impl From<Gate> for DirEntry {
    fn from(gate: Gate) -> Self {
        DirEntry::Gate(gate)
    }
}

pub mod utils {

    use super::*;

    #[derive(Debug)]
    pub enum Error {
        BadPath,
        CorruptedMemsizeFile,
        LabelError(LabelError),
        FacetedDir(FacetedDirectory, Buckle),
        GateError(GateError),
        LinkError(LinkError),
        UnlinkError(UnlinkError),
        FacetError(FacetError),
    }

    impl From<LabelError> for Error {
        fn from(err: LabelError) -> Self {
            Error::LabelError(err)
        }
    }

    impl From<GateError> for Error {
        fn from(err: GateError) -> Self {
            Error::GateError(err)
        }
    }

    impl From<LinkError> for Error {
        fn from(err: LinkError) -> Self {
            Error::LinkError(err)
        }
    }

    impl From<UnlinkError> for Error {
        fn from(err: UnlinkError) -> Self {
            Error::UnlinkError(err)
        }
    }

    impl From<FacetError> for Error {
        fn from(err: FacetError) -> Self {
            Error::FacetError(err)
        }
    }

    pub fn _read_path<S: Clone + BackingStore>(
        fs: &FS<S>,
        path: self::path::Path,
        clearance_checker: fn() -> bool,
    ) -> Result<DirEntry, Error> {
        use self::path::Component as PC;
        if let Some((last, path)) = path.split_last() {
            let second_last =
                path.iter()
                    .try_fold(fs.root().into(), |de, comp| -> Result<DirEntry, Error> {
                        let res = match de {
                            super::DirEntry::Directory(dir) => {
                                // implicitly raising the label
                                taint_with_label(dir.label.clone());
                                match comp {
                                    PC::Dscrp(s) => {
                                        fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath)
                                    }
                                    _ => Err(Error::BadPath),
                                }
                            }
                            super::DirEntry::FacetedDirectory(fdir) => {
                                match comp {
                                    PC::Facet(f) => {
                                        // implicitly raising the label
                                        taint_with_label(f.clone());
                                        fs.open_facet(&fdir, f)
                                            .map(|d| DirEntry::Directory(d))
                                            .map_err(|e| Error::from(e))
                                    }
                                    _ => Err(Error::BadPath),
                                }
                            }
                            super::DirEntry::Blob(_)
                            | super::DirEntry::Gate(_)
                            | super::DirEntry::File(_) => Err(Error::BadPath),
                        };
                        if res.is_ok() && !clearance_checker() {
                            return Err(Error::LabelError(LabelError::CannotRead));
                        }
                        res
                    })?;
            // corner case: the last component is an unallocated facet.
            let res = match second_last {
                super::DirEntry::Directory(dir) => {
                    // implicitly raising the label
                    taint_with_label(dir.label.clone());
                    match last {
                        PC::Dscrp(s) => {
                            fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath)
                        }
                        _ => Err(Error::BadPath),
                    }
                }
                super::DirEntry::FacetedDirectory(fdir) => {
                    match last {
                        PC::Facet(f) => {
                            // implicitly raising the label
                            taint_with_label(f.clone());
                            match fs.open_facet(&fdir, f) {
                                Ok(d) => Ok(DirEntry::Directory(d)),
                                Err(FacetError::Unallocated) => {
                                    Err(Error::FacetedDir(fdir, f.clone()))
                                }
                                Err(e) => Err(Error::from(e)),
                            }
                        }
                        _ => Err(Error::BadPath),
                    }
                }
                super::DirEntry::Blob(_) | super::DirEntry::Gate(_) | super::DirEntry::File(_) => {
                    Err(Error::BadPath)
                }
            };
            if res.is_ok() && !clearance_checker() {
                return Err(Error::LabelError(LabelError::CannotRead));
            }
            res
        } else {
            // corner case: empty vector is the root's path
            Ok(fs.root().into())
        }
    }

    pub fn read_path<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<DirEntry, Error> {
        _read_path(fs, path.into(), noop)
    }

    pub fn read_path_check_clearance<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<DirEntry, Error> {
        _read_path(fs, path.into(), check_clearance)
    }

    pub fn list<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<HashMap<String, DirEntry>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::Directory(dir)) => fs.list(dir).map_err(|e| Error::from(e)),
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn faceted_list<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<HashMap<String, HashMap<String, DirEntry>>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::FacetedDirectory(fdir)) => Ok(fs.faceted_list(&fdir)),
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn read<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<Vec<u8>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::File(f)) => {
                taint_with_label(f.label.clone());
                fs.read(&f).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn open_blob<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<String, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::Blob(b)) => {
                taint_with_label(b.label.clone());
                fs.open_blob(&b).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn update_blob<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
        blobname: String,
    ) -> Result<(), Error> {
        match read_path(fs, path) {
            Ok(DirEntry::Blob(b)) => {
                endorse_with_full();
                fs.update_blob(&b, blobname).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn write<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
        data: Vec<u8>,
    ) -> Result<(), Error> {
        match read_path(fs, path) {
            Ok(DirEntry::File(file)) => {
                endorse_with_full();
                fs.write(&file, &data).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn delete<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
    ) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(DirEntry::Directory(dir)) => {
                endorse_with_full();
                fs.unlink(&dir, name)
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Ok(DirEntry::FacetedDirectory(fdir)) => {
                endorse_with_full();
                fs.faceted_unlink(&fdir, name)
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(e),
        }
    }

    pub fn create_redirect_gate<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        policy: Buckle,
        redirect_path: P,
    ) -> Result<(), Error> {
        _create_gate(fs, base_dir, name, policy, Some(redirect_path), None)
    }

    pub fn create_gate<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        policy: Buckle,
        f: Function,
    ) -> Result<(), Error> {
        _create_gate(fs, base_dir, name, policy, None, Some(f))
    }

    fn _create_gate<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        policy: Buckle,
        redirect_path: Option<P>,
        f: Option<Function>,
    ) -> Result<(), Error> {
        let create_gate = || -> Result<Gate, Error> {
            if let Some(f) = f {
                fs.create_gate(policy.secrecy, policy.integrity, f)
                    .map_err(|e| Error::from(e))
            } else {
                fs.create_redirect_gate(
                    policy.secrecy,
                    policy.integrity,
                    redirect_path.unwrap().into(),
                )
                .map_err(|e| Error::from(e))
            }
        };
        match read_path(&fs, base_dir) {
            Ok(DirEntry::Directory(dir)) => {
                let gate = create_gate()?;
                endorse_with_full();
                fs.link(&dir, name, DirEntry::Gate(gate))
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Ok(DirEntry::FacetedDirectory(fdir)) => {
                let gate = create_gate()?;
                endorse_with_full();
                fs.faceted_link(&fdir, None, name, DirEntry::Gate(gate))
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Err(Error::FacetedDir(fdir, facet)) => {
                let gate = create_gate()?;
                endorse_with_full();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Gate(gate))
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(e),
        }
    }

    pub fn dup_gate<S: BackingStore + Clone, P: Into<self::path::Path>>(
        fs: &FS<S>,
        orig: P,
        base_dir: P,
        name: String,
        policy: Buckle,
    ) -> Result<(), Error> {
        let dpriv = policy.secrecy;
        let wpr = policy.integrity;
        if !check_delegation(&dpriv) {
            return Err(Error::from(GateError::CannotDelegate));
        }
        read_path(fs, orig).and_then(|entry| match entry {
            DirEntry::Gate(orig) => {
                let gate = Gate {
                    privilege: dpriv,
                    weakest_privilege_required: wpr,
                    redirect: orig.redirect,
                    object_id: orig.object_id,
                };
                match read_path(fs, base_dir) {
                    Ok(entry) => match entry {
                        DirEntry::Directory(dir) => fs
                            .link(&dir, name, DirEntry::Gate(gate))
                            .map(|_| ())
                            .map_err(|e| Error::from(e)),
                        DirEntry::FacetedDirectory(fdir) => fs
                            .faceted_link(&fdir, None, name, DirEntry::Gate(gate))
                            .map(|_| ())
                            .map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    },
                    Err(Error::FacetedDir(fdir, facet)) => {
                        endorse_with_full();
                        fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Gate(gate))
                            .map(|_| ())
                            .map_err(|e| Error::from(e))
                    }
                    Err(e) => Err(Error::from(e)),
                }
            }
            _ => Err(Error::BadPath),
        })
    }

    pub fn create_directory<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        label: Buckle,
    ) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newdir = fs.create_directory(label);
                    endorse_with_full();
                    fs.link(&dir, name, DirEntry::Directory(newdir))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                DirEntry::FacetedDirectory(fdir) => {
                    let newdir = fs.create_directory(label);
                    endorse_with_full();
                    fs.faceted_link(&fdir, None, name, DirEntry::Directory(newdir))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newdir = fs.create_directory(label);
                endorse_with_full();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Directory(newdir))
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_file<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        label: Buckle,
    ) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newfile = fs.create_file(label);
                    endorse_with_full();
                    fs.link(&dir, name, DirEntry::File(newfile))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                DirEntry::FacetedDirectory(fdir) => {
                    let newfile = fs.create_file(label);
                    endorse_with_full();
                    fs.faceted_link(&fdir, None, name, DirEntry::File(newfile))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newfile = fs.create_file(label);
                endorse_with_full();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::File(newfile))
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_blob<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        label: Buckle,
        blob_name: String,
    ) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let b = fs.create_blob(blob_name, label);
                    endorse_with_full();
                    fs.link(&dir, name, DirEntry::Blob(b))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                DirEntry::FacetedDirectory(fdir) => {
                    let b = fs.create_blob(blob_name, label);
                    endorse_with_full();
                    fs.faceted_link(&fdir, None, name, DirEntry::Blob(b))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let b = fs.create_blob(blob_name, label);
                endorse_with_full();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Blob(b))
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_faceted<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
    ) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newfdir = fs.create_faceted_directory();
                    endorse_with_full();
                    fs.link(&dir, name, DirEntry::FacetedDirectory(newfdir))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                DirEntry::FacetedDirectory(fdir) => {
                    let newfdir = fs.create_faceted_directory();
                    endorse_with_full();
                    fs.faceted_link(&fdir, None, name, DirEntry::FacetedDirectory(newfdir))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newfdir = fs.create_faceted_directory();
                endorse_with_full();
                fs.faceted_link(
                    &fdir,
                    Some(&facet),
                    name,
                    DirEntry::FacetedDirectory(newfdir),
                )
                .map(|_| ())
                .map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn invoke<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<(Function, Component), Error> {
        match read_path(fs, path) {
            Ok(DirEntry::Gate(gate)) => {
                // implicit endorsement
                endorse_with_full();
                if !gate.redirect {
                    fs.invoke(&gate)
                        .map(|f| (f, gate.privilege))
                        .map_err(|e| Error::from(e))
                } else {
                    let redirect_path = fs.invoke_redirect(&gate).map_err(|e| Error::from(e))?;
                    let dircontents = list(fs, redirect_path)?;
                    let app_image = match dircontents.get("app") {
                        Some(DirEntry::Blob(b)) => fs.open_blob(b).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    let runtime_image = match dircontents.get("runtime") {
                        Some(DirEntry::Blob(b)) => fs.open_blob(b).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    let kernel = match dircontents.get("kernel") {
                        Some(DirEntry::Blob(b)) => fs.open_blob(b).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    let raw_memsize = match dircontents.get("memory") {
                        Some(DirEntry::File(f)) => fs.read(f).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    if raw_memsize.len() != 8 {
                        return Err(Error::CorruptedMemsizeFile);
                    }
                    let mut buf = [0u8; 8usize];
                    buf.copy_from_slice(&raw_memsize[0..8]);
                    let memory = usize::from_be_bytes(buf);
                    Ok((
                        Function {
                            memory,
                            app_image,
                            runtime_image,
                            kernel,
                        },
                        gate.privilege,
                    ))
                }
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn invoke_clearance_check<S: Clone + BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<(Function, Component), Error> {
        match read_path_check_clearance(fs, path) {
            Ok(DirEntry::Gate(gate)) => {
                // implicit endorsement
                debug!("invoke gate entry: {:?}", gate);
                endorse_with_full();
                if !gate.redirect {
                    fs.invoke(&gate)
                        .map(|f| (f, gate.privilege))
                        .map_err(|e| Error::from(e))
                } else {
                    let redirect_path = fs.invoke_redirect(&gate).map_err(|e| Error::from(e))?;
                    debug!("invoke redirect path: {:?}", redirect_path);
                    let dircontents = list(fs, redirect_path)?;
                    let app_image = match dircontents.get("app") {
                        Some(DirEntry::Blob(b)) => fs.open_blob(b).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    let runtime_image = match dircontents.get("runtime") {
                        Some(DirEntry::Blob(b)) => fs.open_blob(b).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    let kernel = match dircontents.get("kernel") {
                        Some(DirEntry::Blob(b)) => fs.open_blob(b).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    let raw_memsize = match dircontents.get("memory") {
                        Some(DirEntry::File(f)) => fs.read(f).map_err(|e| Error::from(e)),
                        _ => Err(Error::BadPath),
                    }?;
                    if raw_memsize.len() != 8 {
                        return Err(Error::CorruptedMemsizeFile);
                    }
                    let mut buf = [0u8; 8usize];
                    buf.copy_from_slice(&raw_memsize[0..8]);
                    let memory = usize::from_be_bytes(buf);
                    Ok((
                        Function {
                            memory,
                            app_image,
                            runtime_image,
                            kernel,
                        },
                        gate.privilege,
                    ))
                }
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn check_clearance() -> bool {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = CURRENT_LABEL.with(|current_label| {
                CLEARANCE.with(|clearance| current_label.borrow().can_flow_to(&clearance.borrow()))
            });
            stat.borrow_mut().label_tracking += now.elapsed();
            res
        })
    }

    pub fn noop() -> bool {
        true
    }

    pub fn check_delegation(delegated: &Component) -> bool {
        STAT.with(|stat| {
            PRIVILEGE.with(|p| {
                debug!("my_privilege: {:?}, delegated: {:?}", p, delegated);
                let now = Instant::now();
                let res = p.borrow().implies(delegated);
                stat.borrow_mut().label_tracking += now.elapsed();
                res
            })
        })
    }

    pub fn taint_with_secrecy(secrecy: Component) {
        STAT.with(|stat| {
            let now = Instant::now();
            CURRENT_LABEL.with(|current_label| {
                let tainted = current_label
                    .borrow()
                    .clone()
                    .lub(Buckle::new(secrecy, false));
                *current_label.borrow_mut() = tainted;
            });
            stat.borrow_mut().label_tracking += now.elapsed();
        })
    }

    pub fn endorse_with_full() {
        STAT.with(|stat| {
            let now = Instant::now();
            PRIVILEGE.with(|opriv| {
                endorse_with(&*opriv.borrow());
            });
            stat.borrow_mut().label_tracking += now.elapsed();
        })
    }

    pub fn endorse_with(privilege: &Component) {
        STAT.with(|stat| {
            let now = Instant::now();
            CURRENT_LABEL.with(|current_label| {
                let endorsed = current_label.borrow().clone().endorse(privilege);
                *current_label.borrow_mut() = endorsed;
            });
            stat.borrow_mut().label_tracking += now.elapsed();
        })
    }

    pub fn get_current_label() -> Buckle {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = CURRENT_LABEL.with(|l| l.borrow().clone());
            stat.borrow_mut().label_tracking += now.elapsed();
            res
        })
    }

    pub fn get_ufacet() -> Buckle {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = PRIVILEGE.with(|p| Buckle {
                secrecy: p.borrow().clone(),
                integrity: p.borrow().clone(),
            });
            stat.borrow_mut().label_tracking += now.elapsed();
            res
        })
    }

    pub fn taint_with_label(label: Buckle) -> Buckle {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = CURRENT_LABEL.with(|l| {
                let clone = l.borrow().clone();
                *l.borrow_mut() = clone.lub(label);
                l.borrow().clone()
            });
            stat.borrow_mut().label_tracking += now.elapsed();
            res
        })
    }

    pub fn clear_label() {
        STAT.with(|stat| {
            let now = Instant::now();
            CURRENT_LABEL.with(|current_label| {
                *current_label.borrow_mut() = Buckle::public();
            });
            stat.borrow_mut().label_tracking += now.elapsed();
        })
    }

    pub fn my_privilege() -> Component {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = PRIVILEGE.with(|p| p.borrow().clone());
            stat.borrow_mut().label_tracking += now.elapsed();
            res
        })
    }

    pub fn set_my_privilge(newpriv: Component) {
        STAT.with(|stat| {
            let now = Instant::now();
            PRIVILEGE.with(|opriv| {
                *opriv.borrow_mut() = newpriv;
            });
            stat.borrow_mut().label_tracking += now.elapsed();
        })
    }

    pub fn set_clearance(c: Buckle) {
        STAT.with(|stat| {
            let now = Instant::now();
            CLEARANCE.with(|thisc| {
                *thisc.borrow_mut() = c;
            });
            stat.borrow_mut().label_tracking += now.elapsed();
        })
    }

    pub fn declassify(target: Component) -> Result<Buckle, Buckle> {
        STAT.with(|stat| {
            let now = Instant::now();
            let res = CURRENT_LABEL.with(|l| {
                PRIVILEGE.with(|opriv| {
                    if (target.clone() & opriv.borrow().clone()).implies(&l.borrow().secrecy) {
                        Ok(Buckle::new(target, l.borrow().integrity.clone()))
                    } else {
                        Err(l.borrow().clone())
                    }
                })
            });
            stat.borrow_mut().label_tracking += now.elapsed();
            res
        })
    }
}

#[cfg(test)]
mod test {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_collect_garbage() -> Result<(), LabelError> {
        let tmp_dir = TempDir::new().unwrap();

        let dbenv = lmdb::Environment::new()
            .set_map_size(100 * 1024 * 1024 * 1024)
            .set_max_readers(1)
            .open(tmp_dir.path())
            .unwrap();

        utils::taint_with_label(Buckle::top());
        let mut fs = FS::new(&dbenv);
        fs.initialize();

        let objects = vec![
            fs.create_file(Buckle::public()).object_id,
            fs.create_directory(Buckle::public()).object_id,
            fs.create_faceted_directory().object_id,
        ];
        let deleted = fs.collect_garbage()?;

        assert!([objects.clone(), deleted.clone()]
            .concat()
            .iter()
            .map(|x| objects.contains(x) && deleted.contains(x))
            .all(|x| x));

        Ok(())
    }
}
