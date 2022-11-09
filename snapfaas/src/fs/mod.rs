///! Labeled File System


use lmdb::{Transaction, WriteFlags};
use serde::{Serialize, Deserialize};
use std::{collections::HashMap, cell::RefCell, rc::Rc};
use labeled::{dclabel::DCLabel, Label};

pub use errors::*;

thread_local!(static CURRENT_LABEL: RefCell<DCLabel> = RefCell::new(DCLabel::public()));

type UID = u64;

pub trait BackingStore {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<Vec<u8>>;
    fn put<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V);
    fn add<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> bool;
    fn cas<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, expected: Option<V>, value: V) -> Result<(), Option<Vec<u8>>>;
}

impl BackingStore for lmdb::Environment {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<Vec<u8>> {
        let db = self.open_db(None).ok()?;
        let txn = self.begin_ro_txn().ok()?;
        let res = txn.get(db, &key.as_ref()).ok().map(Into::into);
        txn.commit().ok()?;
        res
    }

    fn put<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) {
        let db = self.open_db(None).unwrap();
        let mut txn = self.begin_rw_txn().unwrap();
        let _ = txn.put(db, &key.as_ref(), &value.as_ref(), WriteFlags::empty());
        txn.commit().unwrap();
    }

    fn add<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> bool {
        let db = self.open_db(None).unwrap();
        let mut txn = self.begin_rw_txn().unwrap();
        let res = match txn.put(db, &key.as_ref(), &value.as_ref(), WriteFlags::NO_OVERWRITE) {
            Ok(_) => true,
            Err(_) => false,
        };
        txn.commit().unwrap();
        res
    }

    fn cas<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, expected: Option<V>, value: V) -> Result<(), Option<Vec<u8>>> {
        let db = self.open_db(None).unwrap();
        let mut txn = self.begin_rw_txn().unwrap();
        let old = txn.get(db, &key.as_ref()).ok().map(Into::into);
        let res = if expected.map(|e| Vec::from(e.as_ref())) == old {
            let _ = txn.put(db, &key.as_ref(), &value.as_ref(), WriteFlags::empty());
            Ok(())
        } else {
            Err(old)
        };
        txn.commit().unwrap();
        res
    }
}

pub struct FS<S> {
    storage: Rc<S>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Directory {
    label: DCLabel,
    object_id: UID
}

#[derive(Clone, Serialize, Deserialize)]
pub struct File {
    label: DCLabel,
    object_id: UID,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum DirEntry {
    Directory(Directory),
    File(File)
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
}

impl<S: BackingStore> FS<S> {
    pub fn root(&self) -> Directory {
        Directory {
            label: DCLabel::public(),
            object_id: 0
        }
    }

    pub fn create_directory(&self, label: DCLabel) -> Directory {
        let dir_contents = serde_json::ser::to_vec(&HashMap::<String, DirEntry>::new()).unwrap_or((&b"{}"[..]).into());
        let mut uid: UID = rand::random();
        while !self.storage.add(uid.to_be_bytes(), &dir_contents) {
            uid = rand::random();
        }

        Directory {
            label,
            object_id: uid,
        }
    }

    pub fn create_file(&self, label: DCLabel) -> File {
        let mut uid: UID = rand::random();
        while !self.storage.add(uid.to_be_bytes(), &[]) {
            uid = rand::random();
        }
        File {
            label,
            object_id: uid,
        }
    }

    pub fn list(&self, dir: Directory) -> Result<HashMap<String, DirEntry>, LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if dir.label.can_flow_to(&*current_label.borrow()) {
                Ok(match self.storage.get(dir.object_id.to_be_bytes()) {
                    Some(bs) => serde_json::from_slice(bs.as_slice()).unwrap_or_default(),
                    None => Default::default()
                })
            } else {
                Err(LabelError::CannotRead)
            }
        })
    }

    pub fn link(&self, dir: &Directory, name: String, direntry: DirEntry) -> Result<String, LinkError>{
        CURRENT_LABEL.with(|current_label| {
            if !current_label.borrow().secrecy.implies(&dir.label.secrecy) {
                return Err(LinkError::LabelError(LabelError::CannotRead));
            }
            if !current_label.borrow().can_flow_to(&dir.label) {
                return Err(LinkError::LabelError(LabelError::CannotWrite));
            }
            let mut raw_dir: Option<Vec<u8>> = self.storage.get(dir.object_id.to_be_bytes());
            loop {
                let mut dir_contents: HashMap<String, DirEntry> = raw_dir.as_ref().and_then(|dir_contents| serde_json::from_slice(dir_contents.as_slice()).ok()).unwrap_or_default();
                if let Some(_) = dir_contents.insert(name.clone(), direntry.clone()) {
                    return Err(LinkError::Exists)
                }
                match self.storage.cas(dir.object_id.to_be_bytes(), raw_dir, serde_json::to_vec(&dir_contents).unwrap_or_default()) {
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
            let mut raw_dir = self.storage.get(dir.object_id.to_be_bytes());
            loop {
                let mut dir_contents: HashMap<String, DirEntry> = raw_dir.as_ref().and_then(|dir_contents| serde_json::from_slice(dir_contents.as_slice()).ok()).unwrap_or_default();
                if dir_contents.remove(&name).is_none() {
                    return Err(UnlinkError::DoesNotExists)
                }
                match self.storage.cas(dir.object_id.to_be_bytes(), raw_dir, serde_json::to_vec(&dir_contents).unwrap_or_default()) {
                    Ok(()) => return Ok(name),
                    Err(rd) => raw_dir = rd,
                }
            }
        })
    }

    pub fn read(&self, file: &File) -> Result<Vec<u8>, LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if file.label.can_flow_to(&*current_label.borrow()) {
                Ok(self.storage.get(file.object_id.to_be_bytes()).unwrap_or_default())
            } else {
                Err(LabelError::CannotRead)
            }
        })
    }

    pub fn write(&mut self, file: &File, data: &Vec<u8>) -> Result<(), LabelError> {
        CURRENT_LABEL.with(|current_label| {
            if current_label.borrow().can_flow_to(&file.label) {
                Ok(self.storage.put(file.object_id.to_be_bytes(), data))
            } else {
                Err(LabelError::CannotWrite)
            }
        })
    }

}

impl Directory {
    pub fn label(&self) -> &DCLabel {
        &self.label
    }
}

impl File {
    pub fn label(&self) -> &DCLabel {
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

pub mod utils {
    use super::*;

    #[derive(Debug)]
    pub enum Error {
        BadPath,
        LabelError(LabelError),
    }

    impl From<LabelError> for Error {
        fn from(err: LabelError) -> Self {
            Error::LabelError(err)
        }
    }

    pub fn read_path<S: Clone + BackingStore>(fs: &FS<S>, path: Vec<String>) -> Result<DirEntry, Error> {
        path.iter().try_fold(fs.root().into(), |de, comp| -> Result<DirEntry, Error> {
            match de {
                super::DirEntry::Directory(dir) => {
                    fs.list(dir)?.get(comp).map(Clone::clone).ok_or(Error::BadPath)
                },
                super::DirEntry::File(_) => Err(Error::BadPath)
            }
        })
    }
}
