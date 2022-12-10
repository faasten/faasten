///! Labeled File System


use lmdb::{Transaction, WriteFlags};
use log::info;
use serde::{Serialize, Deserialize};
use std::{collections::HashMap, cell::RefCell};
use labeled::{buckle::{Clause, Buckle, Component, Principal}, Label};

pub use errors::*;

use crate::syscalls;

thread_local!(static CURRENT_LABEL: RefCell<Buckle> = RefCell::new(Buckle::public()));
thread_local!(static PRIVILEGE: RefCell<Component> = RefCell::new(Component::dc_true()));
thread_local!(static FD_TABLE: RefCell<HashMap::<i32, DirEntry>> = RefCell::new(HashMap::new()));
thread_local!(static NEXT_FD: RefCell<i32> = RefCell::new(1i32));

type UID = u64;

pub struct OpaqueHandle(i32);

impl Into<OpaqueHandle> for syscalls::OpaqueHandle {
    fn into(self) -> OpaqueHandle {
        OpaqueHandle(self.inner)
    }
}

impl OpaqueHandle {
    pub fn new(hard_link: DirEntry) -> Self {
        FD_TABLE.with(|tab| {
            NEXT_FD.with(|fd| {
                let newfd = *fd.borrow();
                *fd.borrow_mut() = newfd + 1;
                tab.borrow_mut().insert(newfd, hard_link);
                OpaqueHandle(newfd)
            })
        })
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
        let db = self.open_db(None).ok()?;
        let txn = self.begin_ro_txn().ok()?;
        let res = txn.get(db, &key).ok().map(Into::into);
        txn.commit().ok()?;
        res
    }

    fn put(&self, key: &[u8], value: &[u8]) {
        let db = self.open_db(None).unwrap();
        let mut txn = self.begin_rw_txn().unwrap();
        let _ = txn.put(db, &key, &value, WriteFlags::empty());
        txn.commit().unwrap();
    }

    fn add(&self, key: &[u8], value: &[u8]) -> bool {
        let db = self.open_db(None).unwrap();
        let mut txn = self.begin_rw_txn().unwrap();
        let res = match txn.put(db, &key, &value, WriteFlags::NO_OVERWRITE) {
            Ok(_) => true,
            Err(_) => false,
        };
        txn.commit().unwrap();
        res
    }

    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8]) -> Result<(), Option<Vec<u8>>> {
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
        res
    }
}

#[derive(Debug)]
pub struct FS<S> {
    storage: S,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Directory {
    label: Buckle,
    object_id: UID
}

#[derive(Clone, Serialize, Deserialize)]
pub struct File {
    label: Buckle,
    object_id: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetedDirectory {
    object_id: UID
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct FacetedDirectoryInner {
    facets: Vec<Directory>,
    // allocated lookup
    allocated: HashMap::<String, usize>,
    // indexing for single principals, they own categories they compose
    principal_indexing: HashMap::<Principal, Vec<usize>>,
}

impl FacetedDirectoryInner {
    pub fn open_facet(&self, facet: &Buckle) -> Result<Directory, FacetError> {
        let jsonfacet = serde_json::to_string(facet).unwrap();
        CURRENT_LABEL.with(|current_label| {
            if facet.can_flow_to(&*current_label.borrow()) {
                Ok(self.allocated.get(&jsonfacet).map(|idx| -> Directory {
                    self.facets.get(idx.clone()).unwrap().clone()
                }).ok_or(FacetError::Unallocated))
            } else {
                Err(FacetError::LabelError(LabelError::CannotRead))
            }
        })?
    }

    pub fn dummy_list_facets(&self) -> Vec<Directory> {
        CURRENT_LABEL.with(|current_label| {
            self.facets.iter().filter(|d| d.label.can_flow_to(&*current_label.borrow())).cloned().collect()
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
                            if p.len() == 1 {
                                return self.principal_indexing.get(p.first().unwrap()).map(|v|
                                    v.iter().fold(Vec::new(), |mut dirs, idx| {
                                        dirs.push(self.facets[idx.clone()].clone()); dirs}))
                                    .unwrap_or_default()
                            }
                        }
                    }
                    return self.dummy_list_facets()
                }
                Component::DCFalse => self.dummy_list_facets(),
            }
        })
    }

    pub fn append(&mut self, dir: Directory) -> Option<Directory> {
        let facet = serde_json::ser::to_string(&dir.label).unwrap();
        match self.allocated.get(&facet) {
            Some(idx) => Some(self.facets[idx.clone()].clone()),
            None => {
                self.facets.push(dir.clone());
                let idx = self.facets.len()-1;
                self.allocated.insert(facet, idx);
                // update principal_indexing
                match dir.label.secrecy {
                    Component::DCFormula(clauses) => {
                        if clauses.len() == 1 {
                            let clause = &clauses.iter().next().unwrap().0;
                            for p in clause.iter() {
                                if !self.principal_indexing.contains_key(p.first().unwrap()) {
                                    self.principal_indexing.insert(p.first().unwrap().clone(), Vec::new());
                                }
                                self.principal_indexing.get_mut(p.first().unwrap()).unwrap().push(idx);
                            }
                        }
                    }
                    Component::DCFalse => (),
                };
                None
            },
        }
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

#[derive(Clone, Serialize, Deserialize)]
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
        let dir_contents = serde_json::ser::to_vec(&HashMap::<String, DirEntry>::new()).unwrap_or((&b"{}"[..]).into());
        let mut uid: UID = rand::random();
        while !self.storage.add(&uid.to_be_bytes(), &dir_contents) {
            uid = rand::random();
        }

        Directory {
            label,
            object_id: uid,
        }
    }

    pub fn create_file(&self, label: Buckle) -> File {
        let mut uid: UID = rand::random();
        while !self.storage.add(&uid.to_be_bytes(), &[]) {
            uid = rand::random();
        }
        File {
            label,
            object_id: uid,
        }
    }

    pub fn create_faceted_directory(&self) -> FacetedDirectory {
        let mut uid: UID = rand::random();
        let empty_faceted_dir = serde_json::ser::to_vec(&FacetedDirectoryInner::default()).unwrap_or((&b"{}"[..]).into());
        while !self.storage.add(&uid.to_be_bytes(), &empty_faceted_dir) {
            uid = rand::random()
        }
        FacetedDirectory {
            object_id: uid,
        }
    }

    pub fn create_gate(&self, dpriv: Component, invoking: Component, image: String) -> Result<Gate, GateError> {
        PRIVILEGE.with(|opriv| {
            if opriv.borrow().implies(&dpriv) {
                let mut uid: UID = rand::random();
                while !self.storage.add(&uid.to_be_bytes(), &[]) {
                    uid = rand::random();
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
    }

    pub fn dup_gate(&self, policy: Buckle, gate: &Gate) -> Result<Gate, GateError> {
        PRIVILEGE.with(|opriv| {
            let dpriv = policy.secrecy;
            if opriv.borrow().implies(&dpriv) {
                let mut uid: UID = rand::random();
                while !self.storage.add(&uid.to_be_bytes(), &[]) {
                    uid = rand::random();
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
    }

    pub fn invoke_gate(&self, gate: &Gate) -> Result<Gate, GateError> {
        CURRENT_LABEL.with(|current_label| {
            PRIVILEGE.with(|opriv| {
                // implicit endorsement
                let clone = current_label.borrow().clone();
                *current_label.borrow_mut() = clone.endorse(&*opriv.borrow());
                // check integrity
                if current_label.borrow().integrity.implies(&gate.invoking) {
                    Ok(gate.clone())
                } else {
                    Err(GateError::CannotInvoke)
                }
            })
        })
    }

    pub fn list(&self, dir: Directory) -> Result<HashMap<String, DirEntry>, LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if dir.label.can_flow_to(&*current_label.borrow()) {
                Ok(match self.storage.get(&dir.object_id.to_be_bytes()) {
                    Some(bs) => serde_json::from_slice(bs.as_slice()).unwrap_or_default(),
                    None => Default::default()
                })
            } else {
                println!("dir label: {:?}. current_label: {:?}", dir.label, &*current_label.borrow());
                Err(LabelError::CannotRead)
            }
        })
    }

    pub fn faceted_list(&self, fdir: FacetedDirectory) -> HashMap<String, HashMap<String, DirEntry>> {
        match self.storage.get(&fdir.object_id.to_be_bytes()) {
            Some(bs) => {
                serde_json::from_slice::<FacetedDirectoryInner>(bs.as_slice())
                    .map(|inner| inner.list_facets()).unwrap_or_default().iter()
                    .fold(HashMap::<String, HashMap<String, DirEntry>>::new(),
                    |mut m, dir| {
                        m.insert(serde_json::ser::to_string(dir.label()).unwrap(),self.list(dir.clone()).unwrap());
                        m
                    })
            }
            None => Default::default(),
        }
    }

    fn open_facet(&self, fdir: &FacetedDirectory, facet: &Buckle) -> Result<Directory, FacetError> {
        match self.storage.get(&fdir.object_id.to_be_bytes()) {
            Some(bs) => {
                let inner: FacetedDirectoryInner = serde_json::from_slice(bs.as_slice()).map_err(|_| FacetError::Corrupted)?;
                inner.open_facet(facet)
            }
            None => Err(FacetError::Corrupted),
        }
    }

    pub fn link(&self, dir: &Directory, name: String, direntry: DirEntry) -> Result<String, LinkError>{
        CURRENT_LABEL.with(|current_label| {
            if !current_label.borrow().secrecy.implies(&dir.label.secrecy) {
                return Err(LinkError::LabelError(LabelError::CannotRead));
            }
            if !current_label.borrow().can_flow_to(&dir.label) {
                return Err(LinkError::LabelError(LabelError::CannotWrite));
            }
            let mut raw_dir: Option<Vec<u8>> = self.storage.get(&dir.object_id.to_be_bytes());
            loop {
                let mut dir_contents: HashMap<String, DirEntry> = raw_dir.as_ref().and_then(|dir_contents| serde_json::from_slice(dir_contents.as_slice()).ok()).unwrap_or_default();
                if let Some(_) = dir_contents.insert(name.clone(), direntry.clone()) {
                    return Err(LinkError::Exists)
                }
                match self.storage.cas(&dir.object_id.to_be_bytes(), raw_dir.as_ref().map(|e| e.as_ref()), &serde_json::to_vec(&dir_contents).unwrap_or_default()) {
                    Ok(()) => return Ok(name),
                    Err(rd) => raw_dir = rd,
                }
            }
        })
    }

    pub fn unlink(&self, dir: &Directory, name: String) -> Result<String, UnlinkError> {
        CURRENT_LABEL.with(|current_label| {
            if !current_label.borrow().secrecy.implies(&dir.label.secrecy) {
                return Err(UnlinkError::LabelError(LabelError::CannotRead));
            }
            if !current_label.borrow().can_flow_to(&dir.label) {
                return Err(UnlinkError::LabelError(LabelError::CannotWrite));
            }
            let mut raw_dir = self.storage.get(&dir.object_id.to_be_bytes());
            loop {
                let mut dir_contents: HashMap<String, DirEntry> = raw_dir.as_ref().and_then(|dir_contents| serde_json::from_slice(dir_contents.as_slice()).ok()).unwrap_or_default();
                if dir_contents.remove(&name).is_none() {
                    return Err(UnlinkError::DoesNotExists)
                }
                match self.storage.cas(&dir.object_id.to_be_bytes(), raw_dir.as_ref().map(|e| e.as_ref()), &serde_json::to_vec(&dir_contents).unwrap_or_default()) {
                    Ok(()) => return Ok(name),
                    Err(rd) => raw_dir = rd,
                }
            }
        })
    }

    pub fn faceted_link(&self, fdir: &FacetedDirectory, facet: Option<&Buckle>, name: String, direntry: DirEntry) -> Result<String, LinkError> {
        CURRENT_LABEL.with(|current_label| {
            // check when facet is specified.
            if facet.is_some() && !current_label.borrow().secrecy.implies(&facet.as_ref().unwrap().secrecy) {
                return Err(LinkError::LabelError(LabelError::CannotRead));
            }
            if facet.is_some() && !current_label.borrow().can_flow_to(&facet.as_ref().unwrap()) {
                return Err(LinkError::LabelError(LabelError::CannotWrite));
            }
            let mut raw_fdir: Option<Vec<u8>> = self.storage.get(&fdir.object_id.to_be_bytes());
            loop{
                let mut fdir_contents: FacetedDirectoryInner = raw_fdir.as_ref().and_then(|fdir_contents| serde_json::from_slice(fdir_contents.as_slice()).ok()).unwrap_or_default();
                match fdir_contents.open_facet(facet.unwrap_or(&*current_label.borrow())) {
                    Ok(dir) => return Ok(self.link(&dir, name.clone(), direntry.clone())?),
                    Err(FacetError::Unallocated) => {
                        println!("allocating facet: {:?}", &*current_label.borrow());
                        let dir = self.create_directory(current_label.borrow().clone());
                        let _ = self.link(&dir, name.clone(), direntry.clone());
                        fdir_contents.append(dir);
                        match self.storage.cas(&fdir.object_id.to_be_bytes(), raw_fdir.as_ref().map(|e| e.as_ref()), &serde_json::to_vec(&fdir_contents).unwrap_or_default()) {
                            Ok(()) => return Ok(name),
                            Err(rd) => raw_fdir = rd,
                        }
                    },
                    Err(_) => panic!("unexpected error."),
                }
            }
        })
    }

    pub fn faceted_unlink(&self, fdir: &FacetedDirectory, name: String) -> Result<String, UnlinkError> {
        CURRENT_LABEL.with(|current_label| {
            let facet = &*current_label.borrow();
            let raw_fdir = self.storage.get(&fdir.object_id.to_be_bytes());
            let fdir_contents: FacetedDirectoryInner = raw_fdir.as_ref().and_then(|fdir_contents| serde_json::from_slice(fdir_contents.as_slice()).ok()).unwrap_or_default();
            match fdir_contents.open_facet(facet) {
                Ok(dir) => return Ok(self.unlink(&dir, name.clone())?),
                Err(FacetError::Unallocated) => return Err(UnlinkError::DoesNotExists),
                Err(_) => panic!("unexpected error."),
            }
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

pub mod label {
}

pub mod handle {
    use super::*;

    pub enum Error {
        InvalidHandle,
        LabelError(LabelError), 
        FacetError(FacetError),
        DoesNotExists,
        NotDirectory,
        NotFacetedDirectory,
    }

    impl From<LabelError> for Error {
        fn from(e: LabelError) -> Self {
            Error::LabelError(e)
        }
    }

    impl From<FacetError> for Error {
        fn from(e: FacetError) -> Self {
            Error::FacetError(e)
        }
    }

    pub fn open_at<S: Clone + BackingStore>(fs: &FS<S>, handle: OpaqueHandle, name: &str) -> Result<OpaqueHandle, Error> {
        FD_TABLE.with(|tab| {
            if let Some(de) = tab.borrow().get(&handle.0) {
                match de {
                    DirEntry::Directory(dir) => {
                        let hm = fs.list(dir.clone()).map_err(|e| Error::from(e))?;
                        let hard_link = hm.get(name).ok_or(Error::DoesNotExists)?;
                        Ok(OpaqueHandle::new(hard_link.clone()))
                    }
                    _ => {
                        Err(Error::NotDirectory)
                    }
                }
            } else {
                Err(Error::InvalidHandle)
            }
        })
    }

    pub fn open_at_facet<S: Clone + BackingStore>(fs: &FS<S>, handle: OpaqueHandle, name: &str, facet: &Buckle) -> Result<OpaqueHandle, Error> {
        FD_TABLE.with(|tab| {
            if let Some(de) = tab.borrow().get(&handle.0) {
                match de {
                    DirEntry::FacetedDirectory(fdir) => {
                        let dir = fs.open_facet(fdir, facet).map_err(|e| Error::from(e))?;
                        let hmap = fs.list(dir).map_err(|e| Error::from(e))?;
                        let hard_link = hmap.get(name).ok_or(Error::DoesNotExists)?;
                        Ok(OpaqueHandle::new(hard_link.clone()))
                    }
                    _ => {
                        Err(Error::NotFacetedDirectory)
                    }
                }
            } else {
                Err(Error::InvalidHandle)
            }
        })
    }
}

pub mod utils {
    use crate::syscalls;

    use super::*;

    #[derive(Debug)]
    pub enum Error {
        BadPath,
        UnallocatedFacet,
        LabelError(LabelError),
        FacetedDir(FacetedDirectory, Buckle),
        GateError(GateError),
        LinkError(LinkError),
        FacetError(FacetError),
    }

    impl From<FacetError> for Error {
        fn from(err: FacetError) -> Self {
            Error::FacetError(err)
        }
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

    pub fn read_path<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<DirEntry, Error> {
        use syscalls::path_component::Component as PC;
        if let Some((last, path)) = path.split_last() {
            let direntry = path.iter().try_fold(fs.root().into(), |de, comp| -> Result<DirEntry, Error> {
                match de {
                    super::DirEntry::Directory(dir) => {
                        // implicitly raising the label
                        taint_with_label(Buckle::new(dir.label.secrecy.clone(), true));
                        match comp.component.as_ref() {
                            Some(PC::Dscrp(s)) => fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath),
                            _ => Err(Error::BadPath),
                        }
                    },
                    super::DirEntry::FacetedDirectory(fdir) => {
                        match comp.component.as_ref() {
                            Some(PC::Facet(f)) => {
                                let facet = crate::vm::pblabel_to_buckle(f);
                                // implicitly raising the label
                                taint_with_label(Buckle::new(facet.secrecy.clone(), true));
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
                    taint_with_label(Buckle::new(dir.label.secrecy.clone(), true));
                    match last.component.as_ref() {
                        Some(PC::Dscrp(s)) => fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath),
                        _ => Err(Error::BadPath),
                    }
                },
                super::DirEntry::FacetedDirectory(fdir) => {
                    match last.component.as_ref() {
                        Some(PC::Facet(f)) => {
                            let facet = crate::vm::pblabel_to_buckle(f);
                            // implicitly raising the secrecy
                            taint_with_label(Buckle::new(facet.secrecy.clone(), true));
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

    pub fn create_gate<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String, policy: Buckle, image: String) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(DirEntry::Directory(dir)) => {
                endorse_with_owned();
                let gate = fs.create_gate(policy.secrecy, policy.integrity, image).map_err(|e| Error::from(e))?;
                fs.link(&dir, name, DirEntry::Gate(gate)).map(|_| ()).map_err(|e| Error::from(e))
            },
            Ok(DirEntry::FacetedDirectory(fdir)) => {
                endorse_with_owned();
                let gate = fs.create_gate(policy.secrecy, policy.integrity, image).map_err(|e| Error::from(e))?;
                fs.faceted_link(&fdir, None, name, DirEntry::Gate(gate)).map(|_| ()).map_err(|e| Error::from(e))
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                endorse_with_owned();
                let gate = fs.create_gate(policy.secrecy, policy.integrity, image).map_err(|e| Error::GateError(e))?;
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Gate(gate)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Ok(_) => Err(Error::BadPath),
            Err(e) => Err(e),
        }
    }

    pub fn create_directory<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String, label: Buckle) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    endorse_with_owned();
                    let newdir = fs.create_directory(label);
                    fs.link(&dir, name, DirEntry::Directory(newdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                DirEntry::FacetedDirectory(fdir) => {
                    endorse_with_owned();
                    let newdir = fs.create_directory(label);
                    fs.faceted_link(&fdir, None, name, DirEntry::Directory(newdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                endorse_with_owned();
                let newdir = fs.create_directory(label);
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::Directory(newdir)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_file<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String, label: Buckle) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    endorse_with_owned();
                    let newfile = fs.create_file(label);
                    fs.link(&dir, name, DirEntry::File(newfile)).map(|_| ()).map_err(|e| Error::from(e))
                },
                DirEntry::FacetedDirectory(fdir) => {
                    endorse_with_owned();
                    let newfile = fs.create_file(label);
                    fs.faceted_link(&fdir, None, name, DirEntry::File(newfile)).map(|_| ()).map_err(|e| Error::from(e))
                },
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                endorse_with_owned();
                let newfile = fs.create_file(label);
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::File(newfile)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_faceted<S: Clone + BackingStore>(fs: &FS<S>, base_dir: &Vec<syscalls::PathComponent>, name: String) -> Result<(), Error> {
        match read_path(&fs, base_dir) {
            Ok(entry) => match entry {
                DirEntry::Directory(dir) => {
                    endorse_with_owned();
                    let newfdir = fs.create_faceted_directory();
                    fs.link(&dir, name, DirEntry::FacetedDirectory(newfdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                DirEntry::FacetedDirectory(fdir) => {
                    endorse_with_owned();
                    let newfdir = fs.create_faceted_directory();
                    fs.faceted_link(&fdir, None, name, DirEntry::FacetedDirectory(newfdir)).map(|_| ()).map_err(|e| Error::from(e))
                },
                _ => Err(Error::BadPath),
            },
            Err(Error::FacetedDir(fdir, facet)) => {
                endorse_with_owned();
                let newfdir = fs.create_faceted_directory();
                fs.faceted_link(&fdir, Some(&facet), name, DirEntry::FacetedDirectory(newfdir)).map(|_| ()).map_err(|e| Error::from(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn open<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<OpaqueHandle, Error> {
        read_path(fs, path).map(|de| {
            OpaqueHandle::new(de)
        })
    }

    pub fn endorse_with_owned() {
        PRIVILEGE.with(|opriv| {
            endorse_with(&*opriv.borrow());
        });
    }

    pub fn endorse_with(privilege: &Component) {
        CURRENT_LABEL.with(|current_label| {
            println!("endorse_with: {:?}.", &privilege);
            let endorsed = current_label.borrow().clone().endorse(privilege);
            println!("endorse_with: after: {:?}.", &endorsed);
            *current_label.borrow_mut() = endorsed;
        })
    }

    pub fn get_current_label() -> Buckle {
        CURRENT_LABEL.with(|l| l.borrow().clone())
    }

    pub fn taint_with_label(label: Buckle) -> Buckle {
        CURRENT_LABEL.with(|l| {
            println!("taint_with_label: {:?}.", label);
            let tainted = l.borrow().clone().lub(label);
            println!("taint_with_label: after: {:?}.", tainted);
            *l.borrow_mut() = tainted;
            l.borrow().clone()
        })
    }

    pub fn clear_local_state() {
        CURRENT_LABEL.with(|current_label| {
            *current_label.borrow_mut() = Buckle::public();
        });
        FD_TABLE.with(|tab| {
            *tab.borrow_mut() = HashMap::new();
        });
        PRIVILEGE.with(|privilege| {
            *privilege.borrow_mut() = Component::dc_true();
        });
        NEXT_FD.with(|fd| {
            *fd.borrow_mut() = 1i32;
        });
    }

    pub fn my_privilege() -> Component {
        PRIVILEGE.with(|p| p.borrow().clone())
    }

    pub fn set_my_privilge(newpriv: Component) {
        PRIVILEGE.with(|opriv| {
            *opriv.borrow_mut() = newpriv;
        });
    }

    pub fn declassify(target: Component) -> Result<Buckle, Buckle> {
        CURRENT_LABEL.with(|l| {
            PRIVILEGE.with(|opriv| {
                if (target.clone() & opriv.borrow().clone()).implies(&l.borrow().secrecy) {
                    Ok(Buckle::new(target, l.borrow().integrity.clone()))
                } else {
                    Err(l.borrow().clone())
                }
            })
        })
    }
}
