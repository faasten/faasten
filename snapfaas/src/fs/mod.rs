///! Labeled File System


use lmdb::{Transaction, WriteFlags};
use log::info;
use serde::{Serialize, Deserialize};
use labeled::{buckle::{Clause, Buckle, Component, Principal}, Label};
use serde_with::serde_as;

use std::{collections::HashMap, cell::RefCell, time::{Duration, Instant}};

use crate::syscall_server::pblabel_to_buckle;

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
    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8]) -> Result<(), Option<Vec<u8>>>;
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

    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8]) -> Result<(), Option<Vec<u8>>> {
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
}

#[derive(Debug)]
pub struct FS<S> {
    storage: S,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directory {
    label: Buckle,
    object_id: UID
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    label: Buckle,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetedDirectory {
    object_id: UID
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FacetedDirectoryInner {
    facets: Vec<Directory>,
    // allocated lookup
    allocated: HashMap::<String, usize>,
    // indexing for single principals, they own categories they compose
    #[serde_as(as = "HashMap<serde_with::json::JsonString, _>")]
    principal_indexing: HashMap::<Vec<Principal>, Vec<usize>>,
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
                    Ok(self.allocated.get(&jsonfacet).map(|idx| -> Directory {
                        self.facets.get(idx.clone()).unwrap().clone()
                    }).ok_or(FacetError::Unallocated))
                } else {
                    Err(FacetError::LabelError(LabelError::CannotRead))
                }
            })?
        })
    }

    pub fn dummy_list_facets(&self) -> Vec<Directory> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                self.facets.iter().filter(|d| {
                    let now = Instant::now();
                    let res = current_label.borrow().secrecy.implies(&d.label.secrecy);
                    stat.borrow_mut().label_tracking += now.elapsed();
                    res
                }).cloned().collect()
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
                            let mut res = self.principal_indexing.get(p).map(|v| v.iter()
                                .map(|idx| self.facets[idx.clone()].clone()).collect::<Vec<Directory>>())
                            .unwrap_or_default();
                            res.extend(self.public_secrecies.iter().map(|idx| self.facets[idx.clone()].clone()));
                            res
                        } else {
                            self.dummy_list_facets()
                        }
                    } else if clauses.len() == 0 {
                        self.public_secrecies.iter().map(|idx| self.facets[idx.clone()].clone()).collect::<Vec<Directory>>()
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
                    let idx = self.facets.len()-1;
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
                                            self.principal_indexing.insert(prefix.clone(), Vec::new());
                                        }
                                        self.principal_indexing.get_mut(&prefix).unwrap().push(idx);
                                    }
                                }
                            } else {
                                // secrecy == dc_true
                                self.public_secrecies.push(idx);
                            }
                        },
                        Component::DCFalse => (),
                    };
                    None
                },
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub privilege: Component,
    invoking: Component,
    // TODO: for now, use the configurations function-name:host-fs-path
    pub image: String,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DirEntry {
    Directory(Directory),
    File(File),
    FacetedDirectory(FacetedDirectory),
    Gate(Gate),
}

mod errors {
    #[derive(Debug)]
    pub enum LinkError {
        LabelError(LabelError),
        Exists
    }

    #[derive(Debug)]
    pub enum UnlinkError {
        LabelError(LabelError),
        DoesNotExists
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
        FS {
            storage
        }
    }
}

impl<S: BackingStore> FS<S> {
    pub fn initialize(&self) {
        let dir_contents = serde_json::ser::to_vec(&HashMap::<String, DirEntry>::new()).unwrap();
        let uid: UID = 0;
        if !self.storage.add(&uid.to_be_bytes(), &dir_contents) {
            info!("Existing root directory found.")
        }
    }

    pub fn root(&self) -> Directory {
        let sys_principal = Vec::<String>::new();
        Directory {
            label: Buckle::new(true, [Clause::new_from_vec(vec![sys_principal])]),
            object_id: 0
        }
    }

    pub fn create_directory(&self, label: Buckle) -> Directory {
        STAT.with(|stat| {
            let now = Instant::now();
            let dir_contents = serde_json::ser::to_vec(&HashMap::<String, DirEntry>::new()).unwrap_or((&b"{}"[..]).into());
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
            let empty_faceted_dir = serde_json::ser::to_vec(&FacetedDirectoryInner::default()).unwrap();
            stat.borrow_mut().ser_faceted += now.elapsed();
            while !self.storage.add(&uid.to_be_bytes(), &empty_faceted_dir) {
                uid = rand::random()
            }
            FacetedDirectory {
                object_id: uid,
            }
        })
    }

    pub fn create_gate(&self, dpriv: Component, invoking: Component, image: String) -> Result<Gate, GateError> {
        PRIVILEGE.with(|opriv| {
            STAT.with(|stat| {
                let now = Instant::now();
                if opriv.borrow().implies(&dpriv) {
                    stat.borrow_mut().label_tracking += now.elapsed();
                    let mut uid: UID = rand::random();
                    while !self.storage.add(&uid.to_be_bytes(), &[]) {
                        uid = rand::random();
                        stat.borrow_mut().create_retry += 1;
                    }
                    Ok(Gate{
                        privilege: dpriv,
                        invoking,
                        image,
                        object_id: uid
                    })
                } else {
                    Err(GateError::CannotDelegate)
                }
            })
        })
    }

    pub fn dup_gate(&self, policy: Buckle, gate: &Gate) -> Result<Gate, GateError> {
        PRIVILEGE.with(|opriv| {
            STAT.with(|stat| {
                let dpriv = policy.secrecy;
                let now = Instant::now();
                if opriv.borrow().implies(&dpriv) {
                    stat.borrow_mut().label_tracking += now.elapsed();
                    let mut uid: UID = rand::random();
                    while !self.storage.add(&uid.to_be_bytes(), &[]) {
                        uid = rand::random();
                        stat.borrow_mut().create_retry += 1;
                    }
                    Ok(Gate{
                        privilege: dpriv,
                        invoking: policy.integrity,
                        image: gate.image.clone(),
                        object_id: uid
                    })
                } else {
                    Err(GateError::CannotDelegate)
                }
            })
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

    pub fn faceted_list(&self, fdir: &FacetedDirectory) -> HashMap<String, HashMap<String, DirEntry>> {
        STAT.with(|stat| {
            match self.storage.get(&fdir.object_id.to_be_bytes()) {
                Some(bs) => {
                        let now = Instant::now();
                        serde_json::from_slice::<FacetedDirectoryInner>(bs.as_slice())
                            .map(|inner| {
                                stat.borrow_mut().de_faceted += now.elapsed();
                                inner.list_facets()
                            }).unwrap_or_default().iter()
                            .fold(HashMap::<String, HashMap<String, DirEntry>>::new(),
                            |mut m, dir| {
                                let now = Instant::now();
                                m.insert(serde_json::ser::to_string(dir.label()).unwrap(),self.list(dir.clone()).unwrap());
                                stat.borrow_mut().ser_label += now.elapsed();
                                m
                            })
                }
                None => Default::default(),
            }
        })
    }

    fn open_facet(&self, fdir: &FacetedDirectory, facet: &Buckle) -> Result<Directory, FacetError> {
        STAT.with(|stat| {
            match self.storage.get(&fdir.object_id.to_be_bytes()) {
                Some(bs) => {
                    let now = Instant::now();
                    let inner: FacetedDirectoryInner = serde_json::from_slice(bs.as_slice()).map_err(|_| FacetError::Corrupted)?;
                    stat.borrow_mut().de_faceted += now.elapsed();
                    inner.open_facet(facet)
                }
                None => Err(FacetError::NoneValue),
            }
        })
    }

    pub fn link(&self, dir: &Directory, name: String, direntry: DirEntry) -> Result<String, LinkError>{
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
                    let mut dir_contents: HashMap<String, DirEntry> = raw_dir.as_ref().and_then(|dir_contents| {
                        let now = Instant::now();
                        let res = serde_json::from_slice(dir_contents.as_slice()).ok();
                        stat.borrow_mut().de_dir += now.elapsed();
                        res
                    }).unwrap_or_default();
                    if let Some(_) = dir_contents.insert(name.clone(), direntry.clone()) {
                        return Err(LinkError::Exists)
                    }
                    let now = Instant::now();
                    let json_vec = serde_json::to_vec(&dir_contents).unwrap_or_default();
                    stat.borrow_mut().ser_dir += now.elapsed();
                    match self.storage.cas(&dir.object_id.to_be_bytes(), raw_dir.as_ref().map(|e| e.as_ref()), &json_vec) {
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
                    let mut dir_contents: HashMap<String, DirEntry> = raw_dir.as_ref().and_then(|dir_contents| {
                        let now = Instant::now();
                        let res = serde_json::from_slice(dir_contents.as_slice()).ok();
                        stat.borrow_mut().de_dir += now.elapsed();
                        res
                    }).unwrap_or_default();
                    if dir_contents.remove(&name).is_none() {
                        return Err(UnlinkError::DoesNotExists)
                    }
                    let now = Instant::now();
                    let json_vec = serde_json::to_vec(&dir_contents).unwrap_or_default();
                    stat.borrow_mut().ser_dir += now.elapsed();
                    match self.storage.cas(&dir.object_id.to_be_bytes(), raw_dir.as_ref().map(|e| e.as_ref()), &json_vec) {
                        Ok(()) => return Ok(name),
                        Err(rd) => raw_dir = rd,
                    }
                }
            })
        })
    }

    pub fn faceted_link(&self, fdir: &FacetedDirectory, facet: Option<&Buckle>, name: String, direntry: DirEntry) -> Result<String, LinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                // check when facet is specified.
                let now = Instant::now();
                if facet.is_some() && !current_label.borrow().secrecy.implies(&facet.as_ref().unwrap().secrecy) {
                    return Err(LinkError::LabelError(LabelError::CannotRead));
                }
                if facet.is_some() && !current_label.borrow().can_flow_to(&facet.as_ref().unwrap()) {
                    return Err(LinkError::LabelError(LabelError::CannotWrite));
                }
                stat.borrow_mut().label_tracking += now.elapsed();
                let mut raw_fdir: Option<Vec<u8>> = self.storage.get(&fdir.object_id.to_be_bytes());
                loop{
                    let mut fdir_contents: FacetedDirectoryInner = raw_fdir.as_ref().and_then(|fdir_contents| {
                        let now = Instant::now();
                        let res = serde_json::from_slice(fdir_contents.as_slice()).ok();
                        stat.borrow_mut().de_faceted += now.elapsed();
                        res
                    }).unwrap_or_default();
                    match fdir_contents.open_facet(facet.unwrap_or(&*current_label.borrow())) {
                        Ok(dir) => return Ok(self.link(&dir, name.clone(), direntry.clone())?),
                        Err(FacetError::Unallocated) => {
                            let dir = self.create_directory(current_label.borrow().clone());
                            let _ = self.link(&dir, name.clone(), direntry.clone());
                            fdir_contents.append(dir);
                            let now = Instant::now();
                            let json_vec = serde_json::to_vec(&fdir_contents).unwrap_or_default();
                            stat.borrow_mut().ser_faceted += now.elapsed();
                            match self.storage.cas(&fdir.object_id.to_be_bytes(), raw_fdir.as_ref().map(|e| e.as_ref()), &json_vec) {
                                Ok(()) => return Ok(name),
                                Err(rd) => raw_fdir = rd,
                            }
                        },
                        Err(_) => panic!("unexpected error."),
                    }
                }
            })
        })
    }

    pub fn faceted_unlink(&self, fdir: &FacetedDirectory, name: String) -> Result<String, UnlinkError> {
        CURRENT_LABEL.with(|current_label| {
            STAT.with(|stat| {
                let facet = &*current_label.borrow();
                let raw_fdir = self.storage.get(&fdir.object_id.to_be_bytes());
                let fdir_contents: FacetedDirectoryInner = raw_fdir.as_ref().and_then(|fdir_contents| {
                    let now = Instant::now();
                    let res = serde_json::from_slice(fdir_contents.as_slice()).ok();
                    stat.borrow_mut().de_faceted += now.elapsed();
                    res
                }).unwrap_or_default();
                match fdir_contents.open_facet(facet) {
                    Ok(dir) => return Ok(self.unlink(&dir, name.clone())?),
                    Err(FacetError::Unallocated) => return Err(UnlinkError::DoesNotExists),
                    Err(_) => panic!("unexpected error."),
                }
            })
        })
    }

    pub fn read(&self, file: &File) -> Result<Vec<u8>, LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if file.label.can_flow_to(&*current_label.borrow()) {
                Ok(self.storage.get(&file.object_id.to_be_bytes()).unwrap_or_default())
            } else {
                Err(LabelError::CannotRead)
            }
        })
    }

    pub fn write(&mut self, file: &File, data: &Vec<u8>) -> Result<(), LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if current_label.borrow().can_flow_to(&file.label) {
                Ok(self.storage.put(&file.object_id.to_be_bytes(), data))
            } else {
                Err(LabelError::CannotWrite)
            }
        })
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
    use crate::syscalls;

    use super::*;

    #[derive(Debug)]
    pub enum Error {
        BadPath,
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

    pub fn read_path<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<DirEntry, Error> {
        use syscalls::path_component::Component as PC;
        taint_with_label(Buckle::public());
        if let Some((last, path)) = path.split_last() {
            let direntry = path.iter().try_fold(fs.root().into(), |de, comp| -> Result<DirEntry, Error> {
                match de {
                    super::DirEntry::Directory(dir) => {
                        // implicitly raising the label
                        taint_with_label(dir.label.clone());
                        match comp.component.as_ref() {
                            Some(PC::Dscrp(s)) => fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath),
                            _ => Err(Error::BadPath),
                        }
                    },
                    super::DirEntry::FacetedDirectory(fdir) => {
                        match comp.component.as_ref() {
                            Some(PC::Facet(f)) => {
                                let facet = pblabel_to_buckle(f);
                                // implicitly raising the label
                                taint_with_label(facet.clone());
                                fs.open_facet(&fdir, &facet).map(|d| DirEntry::Directory(d)).map_err(|e| Error::from(e))
                            },
                            _ => Err(Error::BadPath),
                        }
                    },
                    super::DirEntry::Gate(_) | super::DirEntry::File(_) => Err(Error::BadPath)
                }
            })?;
            // corner case: the last component is an unallocated facet.
            match direntry {
                super::DirEntry::Directory(dir) => {
                    // implicitly raising the label
                    taint_with_label(dir.label.clone());
                    match last.component.as_ref() {
                        Some(PC::Dscrp(s)) => fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath),
                        _ => Err(Error::BadPath),
                    }
                },
                super::DirEntry::FacetedDirectory(fdir) => {
                    match last.component.as_ref() {
                        Some(PC::Facet(f)) => {
                            let facet = pblabel_to_buckle(f);
                            // implicitly raising the label
                            taint_with_label(facet.clone());
                            match fs.open_facet(&fdir, &facet) {
                                Ok(d) => Ok(DirEntry::Directory(d)),
                                Err(FacetError::Unallocated) => Err(Error::FacetedDir(fdir, facet)),
                                Err(e) => Err(Error::from(e)),
                            }
                        },
                        _ => Err(Error::BadPath),
                    }
                },
                super::DirEntry::Gate(_) | super::DirEntry::File(_) => Err(Error::BadPath)
            }
        } else {
            // corner case: empty vector is the root's path
            Ok(fs.root().into())
        }
    }

    pub fn list<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<HashMap<String, DirEntry>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::Directory(dir)) => fs.list(dir).map_err(|e| Error::from(e)),
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn faceted_list<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<HashMap<String, HashMap<String, DirEntry>>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::FacetedDirectory(fdir)) => Ok(fs.faceted_list(&fdir)),
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn read<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<Vec<u8>, Error> {
        match read_path(fs, path) {
            Ok(DirEntry::File(f)) => {
                taint_with_label(f.label.clone());
                fs.read(&f).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn write<S: Clone + BackingStore>(fs: &mut FS<S>, path: &Vec<syscalls::PathComponent>, data: Vec<u8>) -> Result<(), Error> {
        match read_path(fs, path) {
            Ok(DirEntry::File(file)) => {
                endorse_with_owned();
                fs.write(&file, &data).map_err(|e| Error::from(e))
            },
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn delete<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String) -> Result<(), Error> {
        // raise the integrity to true
        match read_path(&fs, base_dir) {
            Ok(DirEntry::Directory(dir)) => {
                endorse_with_owned();
                fs.unlink(&dir, name).map(|_| ()).map_err(|e| Error::from(e))
            }
            Ok(DirEntry::FacetedDirectory(fdir)) => {
                endorse_with_owned();
                fs.faceted_unlink(&fdir, name).map(|_| ()).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(e),
        }
    }

    pub fn create_gate<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String, policy: Buckle, image: String) -> Result<(), Error> {
        // raise the integrity to true
        match read_path(&fs, base_dir) {
            Ok(DirEntry::Directory(dir)) => {
                let gate = fs.create_gate(policy.secrecy, policy.integrity, image).map_err(|e| Error::from(e))?;
                endorse_with_owned();
                fs.link(&dir, name, DirEntry::Gate(gate)).map(|_| ()).map_err(|e| Error::from(e))
            },
            Ok(DirEntry::FacetedDirectory(fdir)) => {
                endorse_with_owned();
                let gate = fs.create_gate(policy.secrecy, policy.integrity, image).map_err(|e| Error::from(e))?;
                fs.faceted_link(&fdir, None, name, DirEntry::Gate(gate)).map(|_| ()).map_err(|e| Error::from(e))
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let gate = fs.create_gate(policy.secrecy, policy.integrity, image).map_err(|e| Error::GateError(e))?;
                endorse_with_owned();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Gate(gate)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(e),
        }
    }

    pub fn create_directory<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String, label: Buckle) -> Result<(), Error> {
        // raise the integrity to true
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newdir = fs.create_directory(label);
                    endorse_with_owned();
                    fs.link(&dir, name, DirEntry::Directory(newdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                DirEntry::FacetedDirectory(fdir) => {
                    let newdir = fs.create_directory(label);
                    endorse_with_owned();
                    fs.faceted_link(&fdir, None, name, DirEntry::Directory(newdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newdir = fs.create_directory(label);
                endorse_with_owned();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Directory(newdir)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_file<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String, label: Buckle) -> Result<(), Error> {
        // raise the integrity to true
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newfile = fs.create_file(label);
                    endorse_with_owned();
                    fs.link(&dir, name, DirEntry::File(newfile)).map(|_| ()).map_err(|e| Error::from(e))
                },
                DirEntry::FacetedDirectory(fdir) => {
                    let newfile = fs.create_file(label);
                    endorse_with_owned();
                    fs.faceted_link(&fdir, None, name, DirEntry::File(newfile)).map(|_| ()).map_err(|e| Error::from(e))
                },
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newfile = fs.create_file(label);
                endorse_with_owned();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::File(newfile)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_faceted<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String) -> Result<(), Error> {
        // raise the integrity to true
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    let newfdir = fs.create_faceted_directory();
                    endorse_with_owned();
                    fs.link(&dir, name, DirEntry::FacetedDirectory(newfdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                DirEntry::FacetedDirectory(fdir) => {
                    let newfdir = fs.create_faceted_directory();
                    endorse_with_owned();
                    fs.faceted_link(&fdir, None, name, DirEntry::FacetedDirectory(newfdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                let newfdir = fs.create_faceted_directory();
                endorse_with_owned();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::FacetedDirectory(newfdir)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn invoke<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<(String, Component), Error> {
        match read_path(&fs, path) {
            Ok(DirEntry::Gate(gate)) => {
                CURRENT_LABEL.with(|current_label| {
                    PRIVILEGE.with(|opriv| {
                        STAT.with(|stat| {
                            // implicit endorsement
                            endorse_with(&*opriv.borrow());
                            // check integrity
                            let now = Instant::now();
                            if current_label.borrow().integrity.implies(&gate.invoking) {
                                stat.borrow_mut().label_tracking += now.elapsed();
                                Ok((gate.image, gate.privilege))
                            } else {
                                Err(Error::from(GateError::CannotInvoke))
                            }
                        })
                    })
                })
            },
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn taint_with_secrecy(secrecy: Component) {
        STAT.with(|stat| {
            let now = Instant::now();
            CURRENT_LABEL.with(|current_label| {
                let tainted = current_label.borrow().clone().lub(Buckle::new(secrecy, false));
                *current_label.borrow_mut() = tainted;
            });
            stat.borrow_mut().label_tracking += now.elapsed();
        })
    }

    pub fn endorse_with_owned() {
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
