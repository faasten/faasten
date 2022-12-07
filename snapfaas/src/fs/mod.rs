///! Labeled File System


use lmdb::{Transaction, WriteFlags};
use serde::{Serialize, Deserialize};
use std::{collections::HashMap, cell::RefCell};
use labeled::{buckle::{Buckle, Component, Principal}, Label};

pub use errors::*;

thread_local!(static CURRENT_LABEL: RefCell<Buckle> = RefCell::new(Buckle::public()));
thread_local!(static PRIVILEGE: RefCell<Component> = RefCell::new(Component::dc_true()));

type UID = u64;

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
    pub fn open_facet(&self, facet: &Buckle) -> Result<Directory, utils::Error> {
        use utils::Error;
        let jsonfacet = serde_json::to_string(facet).unwrap();
        CURRENT_LABEL.with(|current_label| {
            if facet.can_flow_to(&*current_label.borrow()) {
                Ok(self.allocated.get(&jsonfacet).map(|idx| -> Directory {
                    self.facets.get(idx.clone()).unwrap().clone()
                }).ok_or(Error::UnallocatedFacet))
            } else {
                Err(Error::LabelError(LabelError::CannotRead))
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Gate {
    privilege: Component,
    invoking: Component,
    // TODO: for now, use the configurations function-name:host-fs-path
    image: String,
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
        let dir_contents = serde_json::ser::to_vec(&HashMap::<String, DirEntry>::new()).unwrap_or((&b"{}"[..]).into());
        let uid: UID = 0;
        let _ = self.storage.add(&uid.to_be_bytes(), &dir_contents);
    }

    pub fn root(&self) -> Directory {
        Directory {
            label: Buckle::public(),
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

    pub fn invoke_gate(&self, gate: &Gate) -> Result<String, GateError> {
        CURRENT_LABEL.with(|current_label| {
            if (current_label.borrow()).integrity.implies(&gate.invoking) {
                *current_label.borrow_mut() = current_label.borrow().clone().lub(Buckle::new(true, gate.invoking.clone()));
                PRIVILEGE.with(|opriv| {
                    *opriv.borrow_mut() = gate.privilege.clone();
                });
                Ok(gate.image.clone())
            } else {
                Err(GateError::CannotInvoke)
            }
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

    fn open_facet(&self, fdir: &FacetedDirectory, facet: &Buckle) -> Result<Directory, utils::Error> {
        use utils::Error::BadPath;
        match self.storage.get(&fdir.object_id.to_be_bytes()) {
            Some(bs) => {
                let inner: FacetedDirectoryInner = serde_json::from_slice(bs.as_slice()).map_err(|_| BadPath)?;
                inner.open_facet(facet)
            }
            None => Err(BadPath),
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
            let mut raw_fdir: Option<Vec<u8>> = self.storage.get(&fdir.object_id.to_be_bytes());
            loop{
                let mut fdir_contents: FacetedDirectoryInner = raw_fdir.as_ref().and_then(|fdir_contents| serde_json::from_slice(fdir_contents.as_slice()).ok()).unwrap_or_default();
                match fdir_contents.open_facet(facet.unwrap_or(&*current_label.borrow())) {
                    Ok(dir) => return Ok(self.link(&dir, name.clone(), direntry.clone())?),
                    Err(utils::Error::UnallocatedFacet) => {
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
                Err(utils::Error::UnallocatedFacet) => return Err(UnlinkError::DoesNotExists),
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

pub mod utils {
    use crate::syscalls;

    use super::*;

    #[derive(Debug)]
    pub enum Error {
        BadPath,
        UnallocatedFacet,
        LabelError(LabelError),
        FacetedDir(FacetedDirectory, Buckle),
    }

    impl From<LabelError> for Error {
        fn from(err: LabelError) -> Self {
            Error::LabelError(err)
        }
    }

    pub fn read_path<S: Clone + BackingStore>(fs: &FS<S>, path: &Vec<syscalls::PathComponent>) -> Result<DirEntry, Error> {
        use syscalls::path_component::Component as PC;
        if let Some((last, path)) = path.split_last() {
            let direntry = path.iter().try_fold(fs.root().into(), |de, comp| -> Result<DirEntry, Error> {
                match de {
                    super::DirEntry::Directory(dir) => {
                        match comp.component.as_ref() {
                            Some(PC::Dscrp(s)) => fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath),
                            _ => Err(Error::BadPath),
                        }
                    },
                    super::DirEntry::FacetedDirectory(fdir) => {
                        match comp.component.as_ref() {
                            Some(PC::Facet(f)) => {
                                let facet = crate::vm::pblabel_to_buckle(f);
                                fs.open_facet(&fdir, &facet).map(|d| DirEntry::Directory(d))
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
                    match last.component.as_ref() {
                        Some(PC::Dscrp(s)) => fs.list(dir)?.get(s).map(Clone::clone).ok_or(Error::BadPath),
                        _ => Err(Error::BadPath),
                    }
                },
                super::DirEntry::FacetedDirectory(fdir) => {
                    match last.component.as_ref() {
                        Some(PC::Facet(f)) => {
                            let facet = crate::vm::pblabel_to_buckle(f);
                            match fs.open_facet(&fdir, &facet) {
                                Ok(d) => Ok(DirEntry::Directory(d)),
                                Err(Error::UnallocatedFacet) => Err(Error::FacetedDir(fdir, facet)),
                                Err(e) => Err(e),
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

    pub fn get_current_label() -> Buckle {
        CURRENT_LABEL.with(|l| l.borrow().clone())
    }

    pub fn taint_with_label(label: Buckle) -> Buckle {
        CURRENT_LABEL.with(|l| {
            *l.borrow_mut() = l.borrow().clone().lub(label);
            l.borrow().clone()
        })
    }

    pub fn clear_label() {
        CURRENT_LABEL.with(|current_label| {
            *current_label.borrow_mut() = Buckle::public();
        })
    }

    pub fn my_privilege() -> Component {
        PRIVILEGE.with(|p| p.borrow().clone())
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
