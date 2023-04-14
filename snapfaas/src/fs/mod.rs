///! Labeled File System
pub mod bootstrap;
pub mod lmdb;
pub mod path;
pub mod tikv;

use labeled::{
    buckle::{Buckle, Component, Principal},
    HasPrivilege, Label,
};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use crate::configs::FunctionConfig;

pub use errors::*;

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
    gen_blob: Duration,
    create_dir: Duration,
    create_file: Duration,
    create_faceted: Duration,
    list_dir: Duration,
    list_faceted: Duration,
    read: Duration,
    write: Duration,
    delete: Duration,
    declassify: Duration,
    endorse: Duration,
    taint: Duration,
}

pub mod metrics {
    use super::*;

    pub fn get_stat() -> Metrics {
        STAT.with(|stat| stat.borrow().clone())
    }

    pub fn add_gen_blob_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().gen_blob += elapsed)
    }

    pub fn add_create_dir_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().create_dir += elapsed)
    }

    pub fn add_create_faceted_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().create_faceted += elapsed)
    }

    pub fn add_create_file_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().create_file += elapsed)
    }
    pub fn add_list_dir_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().list_dir += elapsed)
    }
    pub fn add_list_faceted_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().list_faceted += elapsed)
    }
    pub fn add_read_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().read += elapsed)
    }
    pub fn add_write_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().write += elapsed)
    }
    pub fn add_delete_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().delete += elapsed)
    }
    pub fn add_declassify_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().declassify += elapsed)
    }
    pub fn add_endorse_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().endorse += elapsed)
    }
    pub fn add_taint_lat(elapsed: Duration) {
        STAT.with(|stat| stat.borrow_mut().taint += elapsed)
    }
}

pub trait BackingStore {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn put(&self, key: &[u8], value: &[u8]);
    fn add(&self, key: &[u8], value: &[u8]) -> bool;
    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8])
        -> Result<(), Option<Vec<u8>>>;
    fn del(&self, key: &[u8]);
    fn get_keys(&self) -> Option<Vec<&[u8]>>;
}

impl<B: BackingStore> BackingStore for &B {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        (*self).get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) {
        (*self).put(key, value)
    }
    fn add(&self, key: &[u8], value: &[u8]) -> bool {
        (*self).add(key, value)
    }
    fn cas(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        value: &[u8],
    ) -> Result<(), Option<Vec<u8>>> {
        (*self).cas(key, expected, value)
    }
    fn del(&self, key: &[u8]) {
        (*self).del(key)
    }
    fn get_keys(&self) -> Option<Vec<&[u8]>> {
        (*self).get_keys()
    }
}

#[derive(Debug)]
pub struct FS<S> {
    storage: S,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directory {
    label: Buckle,
    pub object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    label: Buckle,
    pub object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetedDirectory {
    pub object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    label: Buckle,
    pub object_id: UID,
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
    // a helper function that indexes into the faceted directoryno
    // no label checks, label checks should be done by the caller
    pub fn get_facet(&self, facet: &Buckle) -> Result<Directory, FacetError> {
        STAT.with(|stat| {
            let now = Instant::now();
            let jsonfacet = serde_json::to_string(facet).unwrap();
            stat.borrow_mut().ser_label += now.elapsed();
            self.allocated
                .get(&jsonfacet)
                .map(|idx| -> Directory { self.facets.get(idx.clone()).unwrap().clone() })
                .ok_or(FacetError::Unallocated)
        })
    }

    // iterate over all allocated facets and return visible ones
    pub fn dummy_list_facets(&self) -> Vec<Directory> {
        self.facets
            .iter()
            .filter(|d| checks::can_read_relaxed(&d.label))
            .cloned()
            .collect()
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
    invoker_integrity_clearance: Component,
    redirect: bool,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpVerb {
    GET,
    POST,
    PUT,
    DELETE,
}

impl From<HttpVerb> for reqwest::Method {
    fn from(verb: HttpVerb) -> Self {
        match verb {
            HttpVerb::GET => reqwest::Method::GET,
            HttpVerb::POST => reqwest::Method::POST,
            HttpVerb::PUT => reqwest::Method::PUT,
            HttpVerb::DELETE => reqwest::Method::DELETE,
        }
    }
}

impl From<reqwest::Method> for HttpVerb {
    fn from(method: reqwest::Method) -> Self {
        match method {
            reqwest::Method::GET => HttpVerb::GET,
            reqwest::Method::POST => HttpVerb::POST,
            reqwest::Method::PUT => HttpVerb::PUT,
            reqwest::Method::DELETE => HttpVerb::DELETE,
            _ => panic!("Request method {} not supported", method),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub label: Buckle,
    pub url: String,
    pub verb: HttpVerb,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub privilege: Component,
    weakest_privilege_required: Component,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DirEntry {
    Directory(Directory),
    File(File),
    FacetedDirectory(FacetedDirectory),
    Gate(Gate),
    Blob(Blob),
    Service(Service),
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

    #[derive(Debug)]
    pub enum ServiceError {
        CannotDelegate,
        CannotInvoke,
        Corrupted,
    }
}

mod checks {
    use super::*;
    pub fn can_delegate(delegated: &Component) -> bool {
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

    pub fn can_invoke(wpr: &Component) -> bool {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                debug!("me: {:?}, gate: {:?}", current_label, wpr);
                let now = Instant::now();
                let res = current_label.borrow().integrity.implies(wpr);
                stat.borrow_mut().label_tracking += now.elapsed();
                res
            })
        })
    }

    pub fn can_write(sink: &Buckle) -> bool {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                debug!("me: {:?}, sink: {:?}", current_label, sink);
                let now = Instant::now();
                let res = current_label.borrow().can_flow_to(sink);
                stat.borrow_mut().label_tracking += now.elapsed();
                res
            })
        })
    }

    pub fn can_read(source: &Buckle) -> bool {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                debug!("read source: {:?}, me: {:?}", source, current_label);
                let now = Instant::now();
                let res = source.can_flow_to(&*current_label.borrow());
                stat.borrow_mut().label_tracking += now.elapsed();
                res
            })
        })
    }

    pub fn can_read_relaxed(source: &Buckle) -> bool {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                debug!("read_relaxed source: {:?}, me: {:?}", source, current_label);
                let now = Instant::now();
                let res = current_label.borrow().secrecy.implies(&source.secrecy);
                stat.borrow_mut().label_tracking += now.elapsed();
                res
            })
        })
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

    ///////////////
    /// creates ///
    ///////////////
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

    ////////////////
    /// delegate ///
    ////////////////
    // hard link the redirect target directory
    pub fn create_redirect_gate(
        &self,
        dpriv: Component,
        wpr: Component,
        redirect: Directory,
    ) -> Result<Gate, GateError> {
        if checks::can_delegate(&dpriv) {
            Ok(Gate {
                privilege: dpriv,
                invoker_integrity_clearance: wpr,
                redirect: true,
                object_id: redirect.object_id,
            })
        } else {
            Err(GateError::CannotDelegate)
        }
    }

    pub fn create_gate(
        &self,
        dpriv: Component,
        wpr: Component,
        f: Function,
    ) -> Result<Gate, GateError> {
        STAT.with(|stat| {
            if checks::can_delegate(&dpriv) {
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
                    invoker_integrity_clearance: wpr,
                    redirect: false,
                    object_id: uid,
                })
            } else {
                Err(GateError::CannotDelegate)
            }
        })
    }

    pub fn create_service(
        &self,
        dpriv: Component,
        wpr: Component,
        service_info: ServiceInfo,
    ) -> Service {
        STAT.with(|stat| {
            let mut uid: UID = rand::random();
            let service_info_bytes = serde_json::ser::to_vec(&service_info).unwrap();
            while !self.storage.add(&uid.to_be_bytes(), &service_info_bytes) {
                uid = rand::random();
                stat.borrow_mut().create_retry += 1;
            }
            Service {
                privilege: dpriv,
                weakest_privilege_required: wpr,
                object_id: uid,
            }
        })
    }

    /////////////
    /// reads ///
    /////////////
    pub fn list(&self, dir: &Directory) -> Result<HashMap<String, DirEntry>, LabelError> {
        STAT.with(|stat| {
            if checks::can_read(&dir.label) {
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
                                    self.list(dir).unwrap(),
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

    pub fn open_facet(
        &self,
        fdir: &FacetedDirectory,
        facet: &Buckle,
    ) -> Result<Directory, FacetError> {
        STAT.with(|stat| {
            if checks::can_read(facet) {
                match self.storage.get(&fdir.object_id.to_be_bytes()) {
                    Some(bs) => {
                        let now = Instant::now();
                        let inner: FacetedDirectoryInner = serde_json::from_slice(bs.as_slice())
                            .map_err(|_| FacetError::Corrupted)?;
                        stat.borrow_mut().de_faceted += now.elapsed();
                        inner.get_facet(facet)
                    }
                    None => Err(FacetError::NoneValue),
                }
            } else {
                Err(FacetError::LabelError(LabelError::CannotRead))
            }
        })
    }

    pub fn read(&self, file: &File) -> Result<Vec<u8>, LabelError> {
        if checks::can_read(&file.label) {
            Ok(self
                .storage
                .get(&file.object_id.to_be_bytes())
                .unwrap_or_default())
        } else {
            Err(LabelError::CannotRead)
        }
    }

    pub fn open_blob(&self, blob: &Blob) -> Result<String, LabelError> {
        if checks::can_read(&blob.label) {
            let v = self
                .storage
                .get(&blob.object_id.to_be_bytes())
                .unwrap_or_default();
            Ok(String::from_utf8(v).unwrap_or_default())
        } else {
            Err(LabelError::CannotRead)
        }
    }

    ///////////////////
    /// read-writes ///
    ///////////////////
    pub fn link(
        &self,
        dir: &Directory,
        name: String,
        direntry: DirEntry,
    ) -> Result<String, LinkError> {
        STAT.with(|stat| {
            let now = Instant::now();
            if !checks::can_read_relaxed(&dir.label) {
                return Err(LinkError::LabelError(LabelError::CannotRead));
            }
            if !checks::can_write(&dir.label) {
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
    }

    pub fn unlink(&self, dir: &Directory, name: String) -> Result<String, UnlinkError> {
        STAT.with(|stat| {
            if !checks::can_read_relaxed(&dir.label) {
                return Err(UnlinkError::LabelError(LabelError::CannotRead));
            }
            if !checks::can_write(&dir.label) {
                return Err(UnlinkError::LabelError(LabelError::CannotWrite));
            }
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
    }

    // link for faceted directory, directory link label checks
    pub fn faceted_link(
        &self,
        fdir: &FacetedDirectory,
        facet: Option<&Buckle>,
        name: String,
        direntry: DirEntry,
    ) -> Result<String, LinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                // If facet is None, use the current label to index
                let default_facet = &*current_label.borrow();
                let facet = facet.unwrap_or(default_facet);
                //if !checks::can_read_relaxed(facet) {
                //    return Err(LinkError::LabelError(LabelError::CannotRead));
                //}
                //if !checks::can_write(facet) {
                //    return Err(LinkError::LabelError(LabelError::CannotWrite));
                //}
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
                    match fdir_contents.get_facet(facet) {
                        Ok(dir) => return Ok(self.link(&dir, name.clone(), direntry.clone())?),
                        Err(FacetError::Unallocated) => {
                            let dir = self.create_directory(facet.clone());
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
                        // should never come here
                        Err(e) => panic!("fatal: {:?}", e),
                    }
                }
            })
        })
    }

    // unlink for faceted directory, directory unlink label checks
    pub fn faceted_unlink(
        &self,
        fdir: &FacetedDirectory,
        name: String,
    ) -> Result<String, UnlinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                let facet = &*current_label.borrow();
                if !checks::can_read_relaxed(facet) {
                    return Err(UnlinkError::LabelError(LabelError::CannotRead));
                }
                if !checks::can_write(facet) {
                    return Err(UnlinkError::LabelError(LabelError::CannotWrite));
                }
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
                match fdir_contents.get_facet(facet) {
                    Ok(dir) => self.unlink(&dir, name.clone()),
                    Err(FacetError::Unallocated) => Err(UnlinkError::DoesNotExists),
                    // should never come here
                    Err(e) => panic!("fatal: {:?}", e),
                }
            })
        })
    }

    //////////////
    /// writes ///
    //////////////
    pub fn update_blob(&self, blob: &Blob, blobname: String) -> Result<(), LabelError> {
        if checks::can_write(&blob.label) {
            Ok(self
                .storage
                .put(&blob.object_id.to_be_bytes(), &blobname.into_bytes()))
        } else {
            Err(LabelError::CannotWrite)
        }
    }

    pub fn write(&self, file: &File, data: &Vec<u8>) -> Result<(), LabelError> {
        if checks::can_write(&file.label) {
            Ok(self.storage.put(&file.object_id.to_be_bytes(), data))
        } else {
            Err(LabelError::CannotWrite)
        }
    }

    ///////////////
    /// invokes ///
    ///////////////
    pub fn invoke(&self, gate: &Gate) -> Result<Function, GateError> {
        if checks::can_invoke(&gate.invoker_integrity_clearance) {
            let raw_gate = self
                .storage
                .get(&gate.object_id.to_be_bytes())
                .ok_or(GateError::Corrupted)?;
            Ok(serde_json::from_slice(&raw_gate).map_err(|_| GateError::Corrupted)?)
        } else {
            Err(GateError::CannotInvoke)
        }
    }

    pub fn invoke_redirect(&self, gate: &Gate) -> Result<HashMap<String, DirEntry>, GateError> {
        if checks::can_invoke(&gate.invoker_integrity_clearance) {
            let raw_dir = self
                .storage
                .get(&gate.object_id.to_be_bytes())
                .unwrap_or_default();
            Ok(serde_json::from_slice(&raw_dir).unwrap_or_default())
        } else {
            Err(GateError::CannotInvoke)
        }
    }

    pub fn invoke_service(&self, service: &Service) -> Result<ServiceInfo, ServiceError> {
        CURRENT_LABEL.with(|current_label| {
            if current_label
                .borrow()
                .integrity
                .implies(&service.weakest_privilege_required)
                && current_label
                    .borrow()
                    .can_flow_to_with_privilege(&Buckle::public(), &service.privilege)
            {
                match self.storage.get(&service.object_id.to_be_bytes()) {
                    Some(bs) => {
                        let info = serde_json::from_slice(bs.as_slice()).unwrap();
                        Ok(info)
                    }
                    None => Err(ServiceError::Corrupted),
                }
            } else {
                Err(ServiceError::CannotInvoke)
            }
        })
    }

    //////////
    /// gc ///
    //////////
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
                    let entries = self.list(&dir)?;
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
                DirEntry::Service(service) => {
                    let _ = visited.insert(service.object_id);
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
        MalformedRedirectTarget,
        ClearanceError,
        LabelError(LabelError),
        FacetedDir(FacetedDirectory, Buckle),
        GateError(GateError),
        LinkError(LinkError),
        UnlinkError(UnlinkError),
        FacetError(FacetError),
        ServiceError(ServiceError),
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

    impl From<ServiceError> for Error {
        fn from(err: ServiceError) -> Self {
            Error::ServiceError(err)
        }
    }

    pub fn _read_path<S: BackingStore>(
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
                                    PC::Dscrp(s) => fs
                                        .list(&dir)?
                                        .get(s)
                                        .map(Clone::clone)
                                        .ok_or(Error::BadPath),
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
                            | super::DirEntry::File(_)
                            | super::DirEntry::Service(_) => Err(Error::BadPath),
                        };
                        if res.is_ok() && !clearance_checker() {
                            return Err(Error::ClearanceError);
                        }
                        res
                    })?;
            // corner case: the last component is an unallocated facet.
            let res = match second_last {
                super::DirEntry::Directory(dir) => {
                    // implicitly raising the label
                    taint_with_label(dir.label.clone());
                    match last {
                        PC::Dscrp(s) => fs
                            .list(&dir)?
                            .get(s)
                            .map(Clone::clone)
                            .ok_or(Error::BadPath),
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
                super::DirEntry::Blob(_)
                | super::DirEntry::Gate(_)
                | super::DirEntry::File(_)
                | super::DirEntry::Service(_) => Err(Error::BadPath),
            };
            if res.is_ok() && !clearance_checker() {
                return Err(Error::ClearanceError);
            }
            res
        } else {
            // corner case: empty vector is the root's path
            Ok(fs.root().into())
        }
    }

    pub fn read_path<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<DirEntry, Error> {
        _read_path(fs, path.into(), noop)
    }

    pub fn read_path_check_clearance<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<DirEntry, Error> {
        _read_path(fs, path.into(), check_clearance)
    }

    pub fn list<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<HashMap<String, DirEntry>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::Directory(dir)) => fs.list(&dir).map_err(|e| Error::from(e)),
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    // return entries in all visible facets
    pub fn faceted_list<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<HashMap<String, HashMap<String, DirEntry>>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::FacetedDirectory(fdir)) => Ok(fs.faceted_list(&fdir)),
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn read<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn open_blob<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn update_blob<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn write<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn delete<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn create_redirect_gate<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        policy: Buckle,
        redirect_path: P,
    ) -> Result<(), Error> {
        match read_path(fs, redirect_path) {
            Ok(DirEntry::Directory(d)) => _create_gate(fs, base_dir, name, policy, Some(d), None),
            _ => Err(Error::BadPath),
        }
    }

    pub fn create_gate<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        policy: Buckle,
        f: Function,
    ) -> Result<(), Error> {
        _create_gate(fs, base_dir, name, policy, None, Some(f))
    }

    fn _create_gate<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        policy: Buckle,
        redirect: Option<Directory>,
        f: Option<Function>,
    ) -> Result<(), Error> {
        let create_gate = || -> Result<Gate, Error> {
            if let Some(f) = f {
                fs.create_gate(policy.secrecy, policy.integrity, f)
                    .map_err(|e| Error::from(e))
            } else {
                fs.create_redirect_gate(policy.secrecy, policy.integrity, redirect.unwrap())
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

    // TODO path parse should be done very first to have correct
    // expansion of %
    pub fn dup_gate<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        orig: P,
        base_dir: P,
        name: String,
        policy: Buckle,
    ) -> Result<(), Error> {
        let dpriv = policy.secrecy;
        let wpr = policy.integrity;
        if !checks::can_delegate(&dpriv) {
            return Err(Error::from(GateError::CannotDelegate));
        }
        read_path(fs, orig).and_then(|entry| match entry {
            DirEntry::Gate(orig) => {
                let gate = Gate {
                    privilege: dpriv,
                    invoker_integrity_clearance: wpr,
                    redirect: orig.redirect,
                    object_id: orig.object_id,
                };
                match read_path(fs, base_dir) {
                    Ok(entry) => {
                        endorse_with_full();
                        match entry {
                            DirEntry::Directory(dir) => fs
                                .link(&dir, name, DirEntry::Gate(gate))
                                .map(|_| ())
                                .map_err(|e| Error::from(e)),
                            DirEntry::FacetedDirectory(fdir) => fs
                                .faceted_link(&fdir, None, name, DirEntry::Gate(gate))
                                .map(|_| ())
                                .map_err(|e| Error::from(e)),
                            _ => Err(Error::BadPath),
                        }
                    }
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

    pub fn hard_link<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        dest: P,
        hard_link: DirEntry,
    ) -> Result<(), Error> {
        let dest = dest.into();
        if let Some(base_dir) = dest.parent() {
            if let Some(name) = dest.file_name() {
                return match read_path(fs, base_dir) {
                    Ok(entry) => match entry {
                        DirEntry::Directory(dir) => {
                            endorse_with_full();
                            fs.link(&dir, name, hard_link)
                                .map(|_| ())
                                .map_err(|e| Error::from(e))
                        }
                        DirEntry::FacetedDirectory(fdir) => {
                            endorse_with_full();
                            fs.faceted_link(&fdir, None, name, hard_link)
                                .map(|_| ())
                                .map_err(|e| Error::from(e))
                        }
                        _ => Err(Error::BadPath),
                    },
                    Err(Error::FacetedDir(fdir, facet)) => {
                        endorse_with_full();
                        fs.faceted_link(&fdir, Some(&facet), name, hard_link)
                            .map(|_| ())
                            .map_err(|e| Error::from(e))
                    }
                    Err(e) => Err(e),
                };
            }
        }
        Err(Error::BadPath)
    }

    pub fn create_directory<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn create_file<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        label: Buckle,
    ) -> Result<DirEntry, Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newfile = fs.create_file(label);
                    endorse_with_full();
                    fs.link(&dir, name, DirEntry::File(newfile.clone()))
                        .map(|_| DirEntry::File(newfile))
                        .map_err(|e| Error::from(e))
                }
                DirEntry::FacetedDirectory(fdir) => {
                    let newfile = fs.create_file(label);
                    endorse_with_full();
                    fs.faceted_link(&fdir, None, name, DirEntry::File(newfile.clone()))
                        .map(|_| DirEntry::File(newfile))
                        .map_err(|e| Error::from(e))
                }
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newfile = fs.create_file(label);
                endorse_with_full();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::File(newfile.clone()))
                    .map(|_| DirEntry::File(newfile))
                    .map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_blob<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn create_faceted<S: BackingStore, P: Into<self::path::Path>>(
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

    pub fn create_service<S: BackingStore, P: Into<self::path::Path>, V: Into<self::HttpVerb>>(
        fs: &FS<S>,
        base_dir: P,
        name: String,
        policy: Buckle,
        label: Buckle,
        url: String,
        verb: V,
        headers: HashMap<String, String>,
    ) -> Result<(), Error> {
        let verb = verb.into();
        // raise the integrity to true
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newservice = fs.create_service(
                        policy.secrecy,
                        policy.integrity,
                        ServiceInfo {
                            label,
                            url,
                            verb,
                            headers,
                        },
                    );
                    endorse_with_full();
                    fs.link(&dir, name, DirEntry::Service(newservice))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                DirEntry::FacetedDirectory(fdir) => {
                    let newservice = fs.create_service(
                        policy.secrecy,
                        policy.integrity,
                        ServiceInfo {
                            label,
                            url,
                            verb,
                            headers,
                        },
                    );
                    endorse_with_full();
                    fs.faceted_link(&fdir, None, name, DirEntry::Service(newservice))
                        .map(|_| ())
                        .map_err(|e| Error::from(e))
                }
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newservice = fs.create_service(
                    policy.secrecy,
                    policy.integrity,
                    ServiceInfo {
                        label,
                        url,
                        verb,
                        headers,
                    },
                );
                endorse_with_full();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Service(newservice))
                    .map(|_| ())
                    .map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn invoke<S: BackingStore, P: Into<self::path::Path>>(
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
                    let contents = fs.invoke_redirect(&gate).map_err(|e| Error::from(e))?;
                    let f = parse_redirect_contents(fs, contents, noop)?;
                    Ok((f, gate.privilege))
                }
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn invoke_service<S: BackingStore, P: Into<self::path::Path>>(
        fs: &FS<S>,
        path: P,
    ) -> Result<ServiceInfo, Error> {
        match read_path(&fs, path) {
            Ok(DirEntry::Service(service)) => {
                // implicit endorsement
                endorse_with_full();
                fs.invoke_service(&service).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn invoke_clearance_check<S: BackingStore, P: Into<self::path::Path>>(
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
                    let contents = fs.invoke_redirect(&gate).map_err(|e| Error::from(e))?;
                    let f = parse_redirect_contents(fs, contents, check_clearance)?;
                    Ok((f, gate.privilege))
                }
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    fn parse_redirect_contents<S: BackingStore>(
        fs: &FS<S>,
        contents: HashMap<String, DirEntry>,
        clearance_checker: fn() -> bool,
    ) -> Result<Function, Error> {
        let app_image = match contents.get("app") {
            Some(DirEntry::Blob(b)) => {
                taint_with_label(b.label.clone());
                fs.open_blob(b).map_err(|e| Error::from(e))
            }
            _ => {
                error!("Missing app blob entry");
                Err(Error::MalformedRedirectTarget)
            }
        }?;
        let runtime_image = match contents.get("runtime") {
            Some(DirEntry::Blob(b)) => {
                taint_with_label(b.label.clone());
                fs.open_blob(b).map_err(|e| Error::from(e))
            }
            _ => {
                error!("Missing runtime blob entry");
                Err(Error::MalformedRedirectTarget)
            }
        }?;
        let kernel = match contents.get("kernel") {
            Some(DirEntry::Blob(b)) => {
                taint_with_label(b.label.clone());
                fs.open_blob(b).map_err(|e| Error::from(e))
            }
            _ => {
                error!("Missing kernel blob entry");
                Err(Error::MalformedRedirectTarget)
            }
        }?;
        let raw_memsize = match contents.get("memory") {
            Some(DirEntry::File(f)) => {
                taint_with_label(f.label.clone());
                fs.read(f).map_err(|e| Error::from(e))
            }
            _ => {
                error!("Missing memory file entry");
                Err(Error::MalformedRedirectTarget)
            }
        }?;
        if !clearance_checker() {
            return Err(Error::ClearanceError);
        }
        if raw_memsize.len() != 8 {
            error!("raw_memsize len {}", raw_memsize.len());
            return Err(Error::MalformedRedirectTarget);
        }
        let mut buf = [0u8; 8usize];
        buf.copy_from_slice(&raw_memsize[0..8]);
        let memory = usize::from_be_bytes(buf);
        Ok(Function {
            memory,
            app_image,
            runtime_image,
            kernel,
        })
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

        let dbenv = &*Box::leak(Box::new(
            ::lmdb::Environment::new()
                .set_map_size(100 * 1024 * 1024 * 1024)
                .set_max_readers(1)
                .open(tmp_dir.path())
                .unwrap(),
        ));

        utils::taint_with_label(Buckle::top());
        let mut fs = FS::new(dbenv);
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
