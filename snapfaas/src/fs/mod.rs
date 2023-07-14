use std::collections::BTreeMap;
use std::cell::RefCell;

use labeled::{buckle::{Buckle, Component}, Label, HasPrivilege};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

mod errors;
mod function;

pub mod bootstrap;
pub mod lmdb;
pub mod path;
pub mod tikv;
pub mod utils;

pub use errors::*;
pub use function::*;

use self::path::{Path, PathComponent};

thread_local!(pub(crate) static CURRENT_LABEL: RefCell<Buckle> = RefCell::new(Buckle::public()));
thread_local!(pub(crate) static PRIVILEGE: RefCell<Component> = RefCell::new(Component::dc_true()));

pub const ROOT_REF: ObjectRef<Labeled<Directory>> = ObjectRef::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpVerb {
    HEAD,
    GET,
    POST,
    PUT,
    DELETE,
}

impl From<HttpVerb> for reqwest::Method {
    fn from(verb: HttpVerb) -> Self {
        match verb {
            HttpVerb::HEAD => reqwest::Method::HEAD,
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
pub struct ObjectRef<T> {
    uid: u64,
    _inner: core::marker::PhantomData<T>,
}

// Implement explicitly to avoid relying on T: Copy, as T is just a phantom type
impl<T: Clone> Copy for ObjectRef<T> {}

impl<T> ObjectRef<T> {
    const fn new(uid: u64) -> Self {
        ObjectRef { uid, _inner: core::marker::PhantomData }
    }

    #[allow(dead_code)]
    fn delete<B: BackingStore>(&self, storage: &B) {
        storage.del(&self.uid.to_be_bytes())
    }
}

impl<T: ?Sized + DeserializeOwned> ObjectRef<T> {
    pub fn get<B: BackingStore>(&self, storage: &FS<B>) -> Option<T> {
        let bs = storage.0.get(&self.uid.to_be_bytes())?;
        let res = serde_json::from_slice(bs.as_slice()).ok()?;
        res
    }
}

impl<T: Serialize> ObjectRef<T> {
    fn set_new_id<B: BackingStore>(value: &T, storage: &B) -> ObjectRef<T> {
        let mut uid: u64;
        loop {
            uid = rand::random();
            if storage.add(&uid.to_be_bytes(), &[]) {
                break;
            }
        }

        let res = ObjectRef::new(uid);
        res.set(value, storage);
        res
    }

    fn set<B: BackingStore>(&self, value: &T, storage: &B) {
        storage.put(
            &self.uid.to_be_bytes(),
            serde_json::to_vec(value).unwrap().as_slice(),
        );
    }
}

impl<T: ?Sized + Serialize + DeserializeOwned> ObjectRef<T> {
    fn cas<B: BackingStore>(&self, expected: Option<&T>, value: &T, storage: &B) -> Result<(), Option<T>> {
        let expected: Option<Vec<u8>> = expected.and_then(|e| serde_json::to_vec(e).ok());

        let res = storage.cas(
            &self.uid.to_be_bytes(),
            expected.as_ref().map(Vec::as_slice),
            serde_json::to_vec(value).or(Err(None))?.as_slice(),
        );
        res.map_err(|e| {
            e.and_then(|bs| {
                serde_json::from_slice(bs.as_slice()).ok()
            })
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Labeled<T> {
    label: Buckle,
    data: T,
}

impl<T> Labeled<T> {
    pub fn label(&self) -> &Buckle {
        &self.label
    }

    pub fn unlabel(&self) -> &T {
        CURRENT_LABEL.with(|current_label| {
            let new_label = {
                self.label.clone().lub(current_label.borrow().clone())
            };
            *current_label.borrow_mut() = new_label;
            &self.data
        })
    }

    pub fn write(&mut self, value: T) -> Result<(), errors::LabelError> {
        CURRENT_LABEL.with(|current_label| {
            PRIVILEGE.with(|privilege| {
                if current_label.borrow().can_flow_to_with_privilege(&self.label, &privilege.borrow()) {
                    self.data = value;
                    Ok(())
                } else {
                    Err(errors::LabelError::CannotWrite)
                }
            })
        })
    }

    fn modify<R, F: FnOnce(&mut T) -> R>(&mut self, f: F) -> Result<R, errors::LabelError> {
        CURRENT_LABEL.with(|current_label| {
            let new_label = {
                self.label.clone().lub(current_label.borrow().clone())
            };
            *current_label.borrow_mut() = new_label;
            PRIVILEGE.with(|privilege| {
                if current_label.borrow().can_flow_to_with_privilege(&self.label, &privilege.borrow()) {
                    Ok(f(&mut self.data))
                } else {
                    Err(errors::LabelError::CannotWrite)
                }
            })
        })
    }
}

impl<T: Default + Serialize> ObjectRef<Labeled<T>> {
    pub fn create<B: BackingStore>(label: Buckle, storage: &B) -> Self {
        let labeled = Labeled {
            label,
            data: T::default(),
        };
        ObjectRef::set_new_id(&labeled, storage)
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Directory {
    entries: BTreeMap<String, DirEntry>,
}

impl ObjectRef<Labeled<Directory>> {
    pub fn list<B: BackingStore>(&self, fs: &FS<B>) -> BTreeMap<String, DirEntry> {
        self.get(fs).unwrap().unlabel().entries.clone()
    }

    pub fn link<B: BackingStore>(&self, name: String, entry: DirEntry, fs: &FS<B>) -> Result<bool, errors::LabelError> {
        let mut prev_dir = self.get(fs).unwrap();
        loop {
            let mut labeled_dir = prev_dir.clone();
            let existed = labeled_dir.modify(|dir| {
                dir.entries.insert(name.clone(), entry.clone()).is_some()
            })?;
            if existed {
                return Ok(false);
            }
            if let Err(Some(p)) = self.cas(Some(&prev_dir), &labeled_dir, &fs.0) {
                prev_dir = p;
            } else {
                return Ok(true)
            }
        }
    }

    pub fn unlink<B: BackingStore>(&self, name: &String, fs: &FS<B>) -> Result<bool, errors::LabelError> {
        let mut prev_dir = self.get(fs).unwrap();
        loop {
            let mut labeled_dir = prev_dir.clone();
            let existed = labeled_dir.modify(|dir| {
                dir.entries.remove(name).is_some()
            })?;
            if !existed {
                return Ok(false);
            }
            if let Err(Some(p)) = self.cas(Some(&prev_dir), &labeled_dir, &fs.0) {
                prev_dir = p;
            } else {
                return Ok(true)
            }
        }
    }
}

type File = Vec<u8>;

impl ObjectRef<Labeled<File>> {
    pub fn read<B: BackingStore>(&self, fs: &FS<B>) -> File {
        self.get(fs).unwrap().unlabel().clone()
    }

    pub fn write<B: BackingStore>(&self, data: Vec<u8>, fs: &FS<B>) -> Result<(), errors::LabelError> {
        let mut file = self.get(fs).unwrap();
        file.write(data)?;
        self.set(&file, &fs.0);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FacetedDirectory {
    facets: Vec<(Buckle, ObjectRef<Labeled<Directory>>)>
}

impl ObjectRef<FacetedDirectory> {
    pub fn open<B: BackingStore>(&self, facet: &Buckle, fs: &FS<B>) -> ObjectRef<Labeled<Directory>> {
        let mut mfaceted_dir = self.get(fs);
        loop {
            if let Some(faceted_dir) = mfaceted_dir.as_ref() {
                if let Some(res) = faceted_dir.facets.iter().find_map(|(f, value)| if f.eq(facet) { Some(value) } else { None }) {
                    return *res;
                }
            }
            let new_dir = ObjectRef::set_new_id(&Labeled {
                label: facet.clone(),
                data: Directory::default(),
            }, &fs.0);

            let mut new_faceted_dir = mfaceted_dir.clone().unwrap_or_default();
            new_faceted_dir.facets.push((facet.clone(), new_dir));

            match self.cas(mfaceted_dir.as_ref(), &new_faceted_dir, &fs.0) {
                Ok(()) => return new_dir,
                Err(d) => mfaceted_dir = d.clone(),
            }
        }
    }

    pub fn list<B: BackingStore>(&self, fs: &FS<B>, clearance: &Buckle) -> BTreeMap<Buckle, ObjectRef<Labeled<Directory>>> {
        CURRENT_LABEL.with(|current_label| {
            let cl = {
                current_label.borrow().clone().lub(clearance.clone())
            };
            *current_label.borrow_mut() = cl;
        });
        self.get(fs).unwrap().facets.iter().filter_map(|(label, entry)| {
            if label.can_flow_to(clearance) {
                Some((label.clone(), *entry))
            } else {
                None
            }
        }).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub privilege: Component,
    pub invoker_integrity_clearance: Component,
    pub taint: Buckle,
    pub url: String,
    pub verb: HttpVerb,
    pub headers: BTreeMap<String, String>,
}

impl ObjectRef<Labeled<Service>> {
    pub fn to_invokable<B: BackingStore>(&self, fs: &FS<B>) -> Service {
        self.get(fs).unwrap().unlabel().clone()
    }

    pub fn replace<B: BackingStore>(&self, new_service: Service, fs: &FS<B>) -> Result<(), FsError> {
        PRIVILEGE.with(|privilege| {
            let privilege = privilege.borrow();
            if !privilege.implies(&new_service.privilege) {
                Err(FsError::PrivilegeError(PrivilegeError::CannotDelegate))
            } else {
                Ok(())
            }
        })?;
        let mut service = self.get(fs).unwrap();
        service.write(new_service)?;
        Ok(self.set(&service, &fs.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Gate {
    Direct(DirectGate),
    Redirect(RedirectGate),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedirectGate {
    pub privilege: Component,
    pub invoker_integrity_clearance: Component,
    pub declassify: Component,
    pub gate: ObjectRef<Labeled<Gate>>,
}

impl ObjectRef<Labeled<Gate>> {
    /// Resolves a `RedirectGate` recursively until reaching a direct gate
    ///
    /// At each level, both privilege and `invokable_integrity_clearance` are
    /// accumulated.
    pub fn to_invokable<B: BackingStore>(&self, fs: &FS<B>) -> DirectGate {
        let mut cur = self.get(fs).unwrap().unlabel().clone();
        let mut privilege = Component::dc_true();
        let mut declassify = Component::dc_true();
        let mut invoker_integrity_clearance = Component::dc_true();
        loop {
            match cur {
                Gate::Direct(gate) => {
                    privilege = privilege & gate.privilege;
                    invoker_integrity_clearance = invoker_integrity_clearance & gate.invoker_integrity_clearance;
                    return DirectGate {
                        privilege,
                        invoker_integrity_clearance,
                        declassify,
                        function: gate.function,
                    }
                },
                Gate::Redirect(redirect_gate) => {
                    privilege = privilege & redirect_gate.privilege;
                    invoker_integrity_clearance = invoker_integrity_clearance & redirect_gate.invoker_integrity_clearance;
                    declassify = declassify & redirect_gate.declassify;
                    cur = redirect_gate.gate.get(fs).unwrap().unlabel().clone();
                }
            }
        }
    }

    pub fn replace<B: BackingStore>(&self, new_gate: Gate, fs: &FS<B>) -> Result<(), FsError> {
        {
            let new_priv = match &new_gate {
                Gate::Direct(d) => &d.privilege,
                Gate::Redirect(r) => &r.privilege,
            };
            PRIVILEGE.with(|privilege| {
                let privilege = privilege.borrow();
                if !privilege.implies(new_priv) {
                    Err(FsError::PrivilegeError(PrivilegeError::CannotDelegate))
                } else {
                    Ok(())
                }
            })?;
        }
        let mut gate = self.get(fs).unwrap();
        gate.write(new_gate)?;
        Ok(self.set(&gate, &fs.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectGate {
    pub privilege: Component,
    pub invoker_integrity_clearance: Component,
    pub declassify: Component,
    pub function: Function,
}

impl ObjectRef<Labeled<DirectGate>> {
    pub fn to_invokable<B: BackingStore>(&self, fs: &FS<B>) -> DirectGate {
        self.get(fs).unwrap().unlabel().clone()
    }
}

pub type Blob = String;

impl ObjectRef<Labeled<Blob>> {
    pub fn read<B: BackingStore>(&self, fs: &FS<B>) -> Blob {
        self.get(fs).unwrap().unlabel().clone()
    }

    pub fn replace<B: BackingStore>(&self, new_blob: Blob, fs: &FS<B>) -> Result<(), LabelError> {
        let mut blob = self.get(fs).unwrap();
        blob.write(new_blob)?;
        Ok(self.set(&blob, &fs.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u8)]
pub enum DirEntry {
    Directory(ObjectRef<Labeled<Directory>>) = 0,
    File(ObjectRef<Labeled<File>>) = 1,
    FacetedDirectory(ObjectRef<FacetedDirectory>) = 2,
    Gate(ObjectRef<Labeled<Gate>>) = 3,
    Service(ObjectRef<Labeled<Service>>) = 4,
    Blob(ObjectRef<Labeled<Blob>>) = 5,
}

// FS definition

#[derive(Debug)]
pub struct FS<S: ?Sized>(S);

impl<S> FS<S> {
    pub fn new(storage: S) -> Self {
        Self(storage)
    }
}

impl<S: BackingStore> FS<S> {
    /// true, the root is newly created; false, the root already exists
    pub fn initialize(&self) -> bool {
        let root = Labeled {
            label: Buckle::new(true, false),
            data: Directory {
                entries: Default::default(),
            }
        };
        self.0
            .add(&ROOT_REF.uid.to_be_bytes(), &serde_json::ser::to_vec(&root).unwrap())
    }

    pub fn root(&self) -> Labeled<Directory> {
        ROOT_REF.get(self).unwrap_or(Labeled {
            label: Buckle::new(true, false),
            data: Directory {
                entries: Default::default(),
            }
        })
    }

    /// Returns the directory entry at a path or an error if the path doesn't exist.
    ///
    /// The thread's current label is tainted for each path component, meaning path
    /// traversal never fails when the path exists, but may increase the current
    /// label arbitrarily high.
    pub fn read_path<P: Into<Path>>(
        &self,
        path: P,
    ) -> Result<DirEntry, FsError> {
        let mut path: Path = path.into();

        let mut cur_entry;
        if let Some(PathComponent::Dscrp(comp)) = path.pop_front() {
            cur_entry = self.root().unlabel().entries.get(&comp).cloned();
        } else {
            return Ok(DirEntry::Directory(ROOT_REF));
        }

        while let Some(comp) = path.pop_front() {
            match (cur_entry, comp) {
                (Some(DirEntry::Directory(ref dir_obj)), PathComponent::Dscrp(ref dscrp)) => {
                    cur_entry = dir_obj.list(self).get(dscrp).cloned();
                },
                (Some(DirEntry::FacetedDirectory(ref facet_obj)), PathComponent::Facet(ref facet)) => {
                    cur_entry = Some(DirEntry::Directory(facet_obj.open(facet, self)));
                },
                _ => return Err(FsError::BadPath),
            }
        }
        cur_entry.ok_or(FsError::BadPath)
    }

    /// Lists the contents of a directory
    ///
    /// The thread's current label is tainted for each path component, meaning path
    /// traversal never fails when the path exists, but may increase the current
    /// label arbitrarily high.
    pub fn list_dir<P: Into<Path>>(
        &self,
        path: P,
    ) -> Result<BTreeMap<String, DirEntry>, FsError> {
        match self.read_path(path)? {
            DirEntry::Directory(dir_obj) => {
                Ok(dir_obj.list(self))
            },
            _ => Err(FsError::NotADir)
        }
    }

    /// Unlinks `name` from the directory at `dir`, or returns an error if the
    /// directory doesn't exist or the thread's current label and privilege are
    /// not sufficient to write to the directory.
    ///
    /// The thread's current label is tainted for each path component, meaning path
    /// traversal never fails when the path exists, but may increase the current
    /// label arbitrarily high.
    pub fn rm<P: Into<Path>>(&self, dir: P, name: &String) -> Result<bool, FsError> {
        match self.read_path(dir)? {
            DirEntry::Directory(dir_obj) => {
                dir_obj.unlink(name, self).map_err(Into::into)
            },
            _ => Err(FsError::NotADir)
        }
    }


    /// Lists the contents of a faceted directory up to a clearance label
    ///
    /// The thread's current label is tainted for each path component,
    /// diregarding the provided clearance, meaning path traversal never fails
    /// when the path exists, but may increase the current label arbitrarily
    /// high. The results, though, contain only initialized facets that are
    /// readable up to the provided clearance. If listing the facet is
    /// successful, the current label is always raised to the provided
    /// clearance.
    pub fn list_faceted<P: Into<Path>>(
        &self,
        path: P,
        clearance: &Buckle,
    ) -> Result<BTreeMap<Buckle, ObjectRef<Labeled<Directory>>>, FsError> {
        match self.read_path(path)? {
            DirEntry::FacetedDirectory(dir_obj) => {
                Ok(dir_obj.list(self, clearance))
            },
            _ => Err(FsError::NotADir)
        }
    }

    /// Reads and returns the data of the file at `path`
    ///
    /// The thread's current label is tainted for each path component, meaning path
    /// traversal never fails when the path exists, but may increase the current
    /// label arbitrarily high.
    pub fn read_file<P: Into<Path>>(&self, path: P) -> Result<File, FsError> {
        match self.read_path(path)? {
            DirEntry::File(file_obj) => {
                Ok(file_obj.read(self))
            },
            _ => Err(FsError::NotAFile),
        }
    }

    /// Writes `data` to the file at `path`, or returns an error if the file
    /// doesn't exist or the current thread's label and privilege aren't
    /// sufficient for writing to it.
    ///
    /// The thread's current label is tainted for each path component, meaning path
    /// traversal never fails when the path exists, but may increase the current
    /// label arbitrarily high.
    pub fn write_file<P: Into<Path>>(&self, path: P, data: Vec<u8>) -> Result<(), FsError> {
        match self.read_path(path)? {
            DirEntry::File(file_obj) => {
                file_obj.write(data, self).map_err(Into::into)
            },
            _ => Err(FsError::NotAFile),
        }
    }

    /// Creates an empty file object
    pub fn create_file(&self, label: Buckle) -> DirEntry {
        let new_file = ObjectRef::create(label, &self.0);
        DirEntry::File(new_file)
    }

    /// Creates a labeled Blob object
    pub fn create_blob(&self, label: Buckle, blob_name: String) -> Result<DirEntry, FsError> {
        let new_blob: ObjectRef<Labeled<Blob>> = ObjectRef::create(label, &self.0);
        new_blob.replace(blob_name, self)?;
        Ok(DirEntry::Blob(new_blob))
    }



    /// Creates an empty directory object
    pub fn create_directory(&self, label: Buckle) -> DirEntry {
        let new_dir = ObjectRef::create(label, &self.0);
        DirEntry::Directory(new_dir)
    }

    /// Creates an empty faceted directory object
    pub fn create_faceted_directory(&self) -> DirEntry {
        let new_dir = ObjectRef::set_new_id(&Default::default(), &self.0);
        DirEntry::FacetedDirectory(new_dir)
    }

    pub fn create_direct_gate(&self, label: Buckle, direct_gate: DirectGate) -> Result<DirEntry, FsError> {
        PRIVILEGE.with(|privilege| {
            let privilege = privilege.borrow();
            if !CURRENT_LABEL.with(|current_label| current_label.borrow().can_flow_to_with_privilege(&label, &privilege)) {
                Err(FsError::LabelError(LabelError::CannotWrite))
            } else if !privilege.implies(&direct_gate.privilege) {
                Err(FsError::PrivilegeError(PrivilegeError::CannotDelegate))
            } else if !privilege.implies(&direct_gate.declassify) {
                Err(FsError::PrivilegeError(PrivilegeError::CannotDelegate))
            } else {
                Ok(())
            }
        })?;
        let labeled = Labeled {
            label,
            data: Gate::Direct(direct_gate),
        };
        let new_gate = ObjectRef::set_new_id(&labeled, &self.0);
        Ok(DirEntry::Gate(new_gate))
    }

    pub fn create_redirect_gate(&self, label: Buckle, redirect_gate: RedirectGate) -> Result<DirEntry, FsError> {
        PRIVILEGE.with(|privilege| {
            let privilege = privilege.borrow();
            if !CURRENT_LABEL.with(|current_label| current_label.borrow().can_flow_to_with_privilege(&label, &privilege)) {
                Err(FsError::LabelError(LabelError::CannotWrite))
            } else if !privilege.implies(&redirect_gate.privilege) {
                Err(FsError::PrivilegeError(PrivilegeError::CannotDelegate))
            } else if !privilege.implies(&redirect_gate.declassify) {
                Err(FsError::PrivilegeError(PrivilegeError::CannotDelegate))
            } else {
                Ok(())
            }
        })?;
        let labeled = Labeled {
            label,
            data: Gate::Redirect(redirect_gate),
        };
        let new_gate = ObjectRef::set_new_id(&labeled, &self.0);
        Ok(DirEntry::Gate(new_gate))
    }

    pub fn create_service(&self, label: Buckle, service: Service) -> Result<DirEntry, FsError> {
        PRIVILEGE.with(|privilege| {
            let privilege = privilege.borrow();
            if !CURRENT_LABEL.with(|current_label| current_label.borrow().can_flow_to_with_privilege(&label, &privilege)) {
                Err(FsError::LabelError(LabelError::CannotWrite))
            } else if !privilege.implies(&service.privilege) {
                Err(FsError::PrivilegeError(PrivilegeError::CannotDelegate))
            } else {
                Ok(())
            }
        })?;

        let labeled = Labeled {
            label,
            data: service,
        };
        let new_service = ObjectRef::set_new_id(&labeled, &self.0);
        Ok(DirEntry::Service(new_service))
    }

    /// Links an directory entry in `base_dir`.
    ///
    /// The thread's current label is tainted for each path component, meaning path
    /// traversal never fails when the path exists, but may increase the current
    /// label arbitrarily high.
    pub fn link<P: Into<Path>>(&self, base_dir: P, name: String, direntry: DirEntry) -> Result<(), FsError> {
        match self.read_path(base_dir.into())? {
            DirEntry::Directory(dir_obj) => {
                dir_obj.link(name, direntry, &self).map_err(Into::into).and_then(|success| {
                    if success {
                        Ok(())
                    } else {
                        Err(FsError::NameExists)
                    }
                })
            },
            _ => Err(FsError::NotADir),
        }
    }

    pub fn open_blob<P: Into<Path>>(&self, path: P) -> Result<Blob, FsError> {
        match self.read_path(path)? {
            DirEntry::Blob(blob_obj) => {
                Ok(blob_obj.read(self))
            },
            _ => Err(FsError::NotABlob),
        }
    }

    pub fn replace_blob<P: Into<Path>>(&self, path: P, new_blob: Blob) -> Result<(), FsError> {
        match self.read_path(path)? {
            DirEntry::Blob(blob_obj) => {
                blob_obj.replace(new_blob, self).map_err(Into::into)
            },
            _ => Err(FsError::NotABlob),
        }
    }

}

// Backing store trait

pub trait BackingStore {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn put(&self, key: &[u8], value: &[u8]);
    fn add(&self, key: &[u8], value: &[u8]) -> bool;
    fn cas(&self, key: &[u8], expected: Option<&[u8]>, value: &[u8])
        -> Result<(), Option<Vec<u8>>>;
    fn del(&self, key: &[u8]);
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
}

impl<B: BackingStore + ?Sized> BackingStore for Box<B> {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.as_ref().get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) {
        self.as_ref().put(key, value)
    }
    fn add(&self, key: &[u8], value: &[u8]) -> bool {
        self.as_ref().add(key, value)
    }
    fn cas(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        value: &[u8],
    ) -> Result<(), Option<Vec<u8>>> {
        self.as_ref().cas(key, expected, value)
    }
    fn del(&self, key: &[u8]) {
        self.as_ref().del(key)
    }
}
