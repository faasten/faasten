use rand::{self, RngCore};
use lazy_static;
use lmdb;
use lmdb::{Transaction, WriteFlags};
use labeled::dclabel::DCLabel;

mod dir;
mod file;
mod direntry;
mod faceted;

use self::direntry::{LabeledDirEntry, DirEntry};
use self::dir::Directory;
use self::file::File;
use self::faceted::FacetedDirectory;
use crate::syscalls::PathComponent;

lazy_static::lazy_static! {
    pub static ref DBENV: lmdb::Environment = {
        let dbenv = lmdb::Environment::new()
            .set_map_size(100 * 1024 * 1024 * 1024)
            .open(std::path::Path::new("storage"))
            .unwrap();

        // Create the root directory object at key 0 if not already exists.
        // `put_val_db_no_overwrite` uses `NO_OVERWRITE` as the write flag to make sure that it
        // will be a noop if the root already exists at key 0. And we can safely ignore the
        // returned `Result` here which if an error is an KeyExist error.
        let default_db = dbenv.open_db(None).unwrap();
        let mut txn = dbenv.begin_rw_txn().unwrap();
        let root_uid = 0;
        let _ = put_val_db_no_overwrite(root_uid, FacetedDirectory::new().to_vec(), &mut txn, default_db);
        txn.commit().unwrap();

        dbenv
    };
}

pub enum Error {
    BadPath,
    Unauthorized,
    BadTargetLabel,
}

enum InternalError {
    UnallocatedFacet(u64, FacetedDirectory, String),
    Wrapper(Error),
}

type Result<T> = std::result::Result<T, Error>;

//////////////
//   APIs   //
//////////////
/// read always succeeds by raising labels unless the target path is illegal
pub fn read(path: Vec<PathComponent>, cur_label: &mut DCLabel) -> Result<Vec<u8>> {
    let db = DBENV.open_db(None).unwrap();
    let txn = DBENV.begin_ro_txn().unwrap();
    let res = get_direntry(path, cur_label, &txn, db).and_then(|labeled| -> Result<Vec<u8>> {
        let entry = labeled.unlabel(cur_label);
        match entry.entry_type() {
            DirEntry::F => {
                let file = get_val_db(entry.uid(), &txn, db).map(File::from_vec).unwrap();
                Ok(file.data())
            },
            DirEntry::D | DirEntry::FacetedD => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/// read always succeed by raising labels unless the target path is illegal
pub fn list(path: Vec<PathComponent>, cur_label: &mut DCLabel) -> Result<Vec<String>> {
    let db = DBENV.open_db(None).unwrap();
    let txn = DBENV.begin_ro_txn().unwrap();
    let res = get_direntry(path, cur_label, &txn, db).and_then(|labeled| -> Result<Vec<String>> {
        let entry = labeled.unlabel(cur_label);
        match entry.entry_type() {
            DirEntry::D => {
                let dir = get_val_db(entry.uid(), &txn, db).map(Directory::from_vec).unwrap();
                Ok(dir.list())
            },
            DirEntry::F | DirEntry::FacetedD => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/// create_dir only fails when `cur_label` cannot flow to `label` or target directory's label
pub fn create_dir(base_dir: Vec<PathComponent>, name: &str, label: DCLabel, cur_label: &mut DCLabel) -> Result<()> {
    create_common(base_dir, name, label, cur_label, Directory::new().to_vec(), DirEntry::D)
}

pub fn create_faceted_dir(base_dir: Vec<PathComponent>, name: &str, cur_label: &mut DCLabel) -> Result<()> {
    // It is reasonable to label the faceted directory itself public.
    // Its facets act as security policies.
    create_common(base_dir, name, DCLabel::public(), cur_label, FacetedDirectory::new().to_vec(), DirEntry::FacetedD)
}

/// create_file only fails when `cur_label` cannot flow to `label` or target directory's label
pub fn create_file(base_dir: Vec<PathComponent>, name: &str, label: DCLabel, cur_label: &mut DCLabel) -> Result<()> {
    create_common(base_dir, name, label, cur_label, File::new().to_vec(), DirEntry::F)
}

/// write fails when `cur_label` cannot flow to the target file's label
pub fn write(path: Vec<PathComponent>, data: Vec<u8>, cur_label: &mut DCLabel) -> Result<()> {
    let db = DBENV.open_db(None).unwrap();
    let mut txn = DBENV.begin_rw_txn().unwrap();
    let res = get_direntry(path, cur_label, &txn, db).and_then(|labeled| -> Result<()> {
        let entry = labeled.unlabel_write_check(cur_label)?;
        match entry.entry_type() {
            DirEntry::F => {
                let mut file = get_val_db(entry.uid(), &txn, db).map(File::from_vec).unwrap();
                file.write(data);
                let _ = put_val_db(entry.uid(), file.to_vec(), &mut txn, db);
                Ok(())
            }
            DirEntry::D | DirEntry::FacetedD => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/////////////
// helpers //
/////////////
// return a random u64
fn get_uid() -> u64 {
    let mut ret = rand::thread_rng().next_u64();
    // 0 is reserved by the system for the root directory
    if ret == 0 {
        ret = rand::thread_rng().next_u64();
    }
    ret
}

fn get_val_db<T>(uid: u64, txn: &T, db: lmdb::Database) -> std::result::Result<Vec<u8>, lmdb::Error>
where T: Transaction {
    txn.get(db, &uid.to_be_bytes()).map(Vec::from)
}

fn put_val_db_no_overwrite(uid: u64, val: Vec<u8>, txn: &mut lmdb::RwTransaction, db: lmdb::Database) -> std::result::Result<(), lmdb::Error> {
    txn.put(db, &uid.to_be_bytes(), &val, WriteFlags::NO_OVERWRITE)
}

fn put_val_db(uid: u64, val: Vec<u8>, txn: &mut lmdb::RwTransaction, db: lmdb::Database) -> std::result::Result<(), lmdb::Error> {
    txn.put(db, &uid.to_be_bytes(), &val, WriteFlags::empty())
}

fn path_component_to_string(component: &PathComponent) -> Result<String> {
    use crate::syscalls::path_component::Component::{Facet, Name};
    match &component.component {
        Some(Facet(pb_l)) => serde_json::to_string(&crate::vm::proto_label_to_dc_label(pb_l)).map_err(|_| Error::BadPath),
        Some(Name(s)) => Ok(s.clone()),
        None => panic!("We should not reach here"),
    }
}

// This function is called from read, list, write. It returns the direntry object named by a legal
// `path` argument; otherwise, returns an error.
fn get_direntry<T>(path: Vec<PathComponent>, cur_label: &mut DCLabel, txn: &T, db: lmdb::Database)
    -> Result<LabeledDirEntry>
where T: Transaction
{
    get_direntry_helper(path, cur_label, txn, db, false).map_err(|e| {
        match e {
            InternalError::Wrapper(ret_e) => ret_e,
            _ => panic!("get_direntry should not encounter UnallocatedFacet error."),
        }
    })
}

// This function is called from create_common. It traverses the path and returns the direntry
// object named by a legal `path` argument;otherwise, returns an error. Specifically, it returns
// InternalError::UnallocatedFacet when the traversal encounters a facet that is not yet allocated.
fn get_direntry_allocate(
    path: Vec<PathComponent>,
    cur_label: &mut DCLabel,
    txn: &mut lmdb::RwTransaction,
    db: lmdb::Database
) -> Result<LabeledDirEntry> {
    get_direntry_helper(path, cur_label, txn, db, true).or_else(|e| -> Result<LabeledDirEntry> {
        match e {
            InternalError::UnallocatedFacet(uid, mut faceted, facet) => {
                let label = serde_json::from_str(&facet).unwrap();
                let new_entry = faceted.allocate(&facet, label, cur_label, txn, db)?.clone();
                // update the faceted directory
                let _ = put_val_db(uid, faceted.to_vec(), txn, db);
                Ok(new_entry)
            },
            InternalError::Wrapper(ret_e) => Err(ret_e),
        }
    })
}

// This helper function is used by get_direntry_allocate and get_direntry.
// get_direntry_allocate sets `create` to. get_direntry sets `create` to false.
// When `create` is true and the last path component is an unallocated facet,
// e.g., create_faceted_dir([<true, true>], "public_dir") but the public facet is not yet allocated,
// this function returns UnallocatedFacet error. Note that an unallocated facet occurs but it is
// not the last component, the function returns BadPath meaning the path is legal.
fn get_direntry_helper<T>(path: Vec<PathComponent>, cur_label: &mut DCLabel, txn: &T, db: lmdb::Database, create: bool)
    -> std::result::Result<LabeledDirEntry, InternalError>
where T: Transaction
{
    let mut it = path.iter().peekable();
    // The traversal starts from the root
    let mut labeled = LabeledDirEntry::root();
    while let Some(pb_component) = it.next() {
        let component = path_component_to_string(pb_component).map_err(|e| InternalError::Wrapper(e))?;
        let entry = labeled.unlabel(cur_label);
        match entry.entry_type() {
            DirEntry::F => {
                return Err(InternalError::Wrapper(Error::BadPath));
            },
            DirEntry::D => {
                let cur_dir = get_val_db(entry.uid(), txn, db).map(Directory::from_vec).unwrap();
                labeled = cur_dir.get(&component).map_err(|e| InternalError::Wrapper(e))?.clone();
            },
            DirEntry::FacetedD => {
                let faceted = get_val_db(entry.uid(), txn, db).map(FacetedDirectory::from_vec).unwrap();
                match faceted.get(&component) {
                    Ok(l) => {
                        labeled = l.clone();
                    }
                    Err(e) => {
                        if create && it.peek() == None {
                            // unallocated facet but trailing and called from create
                            return Err(InternalError::UnallocatedFacet(entry.uid(), faceted, component));
                        } else {
                            // unallocated facet that is not trailing or not called from create.
                            return Err(InternalError::Wrapper(e));
                        }
                    }
                };
            },
        };
    }
    Ok(labeled)
}

// This function is called from create_file, create_dir, create_faceted_dir.
fn create_common(
    base_dir: Vec<PathComponent>,
    name: &str,
    label: DCLabel,
    cur_label: &mut DCLabel,
    obj_vec: Vec<u8>,
    entry_type: DirEntry,
) -> Result<()> {
    let db = DBENV.open_db(None).unwrap();
    let mut txn = DBENV.begin_rw_txn().unwrap();
    let res = get_direntry_allocate(base_dir, cur_label, &mut txn, db).and_then(|labeled| -> Result<()> {
        let entry = labeled.unlabel_write_check(cur_label)?;
        match entry.entry_type() {
            DirEntry::D => {
                let mut dir = get_val_db(entry.uid(), &txn, db).map(Directory::from_vec).unwrap();

                let mut uid = get_uid();
                while put_val_db_no_overwrite(uid, obj_vec.clone(), &mut txn, db).is_err() {
                    uid = get_uid();
                }
                // TODO: should check if create is allowed before uid is consumed
                dir.create(name, cur_label, entry_type, label, uid)?;
                let _ = put_val_db(entry.uid(), dir.to_vec(), &mut txn, db);
                Ok(())
            },
            DirEntry::F | DirEntry::FacetedD => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_create_dir_list_fail() {
        // create `/gh_repo`
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = DCLabel::bottom();
        assert!(create_dir("/", "gh_repo", target_label, &mut cur_label).is_ok());

        // list
        let mut cur_label = DCLabel::public();
        assert_eq!(list("/", &mut cur_label).unwrap(), vec![String::from("gh_repo"); 1]);

        // already exists
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = DCLabel::bottom();
        assert_eq!(create_dir("/", "gh_repo", target_label, &mut cur_label).unwrap_err(), Error::BadPath);

        // missing path components
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let mut cur_label = DCLabel::public();
        assert_eq!(create_dir("/gh_repo/yue", "yue", target_label, &mut cur_label).unwrap_err(), Error::BadPath);

        // label too high
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let mut cur_label = target_label.clone();
        assert_eq!(create_dir("/gh_repo", "yue", target_label, &mut cur_label).unwrap_err(), Error::Unauthorized);

        // label too high
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let mut cur_label = DCLabel::new([["yue"]], true);
        assert_eq!(create_dir("/gh_repo", "yue", target_label, &mut cur_label).unwrap_err(), Error::Unauthorized);

        // create /gh_repo/yue
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let mut cur_label = DCLabel::new(true, [["gh_repo"]]);
        assert!(create_dir("/gh_repo", "yue", target_label, &mut cur_label).is_ok());

        // Unauthorized not BadPath
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let mut cur_label = DCLabel::public();
        assert_eq!(create_dir("/gh_repo", "yue", target_label, &mut cur_label).unwrap_err(), Error::Unauthorized);
    }

    #[test]
    fn test_storage_create_file_write_read() {
        // create `/func2`
        let mut cur_label = DCLabel::bottom();
        let target_label = DCLabel::new([["func2"]], [["func2"]]);
        assert!(create_dir("/", "func2", target_label, &mut cur_label).is_ok());

        // create `/func2/mydata.txt`
        // after reading the directory /func2, cur_label gets raised to <func2, func2> and
        // cannot flow to the target label <user2, func2>
        let mut cur_label = DCLabel::new(true, [["func2"]]);
        let target_label = DCLabel::new([["user2"]], [["func2"]]);
        assert_eq!(create_file("/func2", "mydata.txt", target_label, &mut cur_label).unwrap_err(), Error::BadTargetLabel);
        // <func2, func2> can flow to <user2/\func2, func2>
        let target_label = DCLabel::new([["user2"], ["func2"]], [["func2"]]);
        assert!(create_file("/func2", "mydata.txt", target_label, &mut cur_label).is_ok());
        assert_eq!(read("/func2/mydata.txt", &mut cur_label).unwrap(), Vec::<u8>::new());

        // write read
        let text = "test message";
        let data = text.as_bytes().to_vec();
        assert!(write("/func2/mydata.txt", data.clone(), &mut cur_label).is_ok());
        assert_eq!(read("/func2/mydata.txt", &mut cur_label).unwrap(), data);

        //// overwrite read
        let text = "test message test message";
        let data = text.as_bytes().to_vec();
        assert!(write("/func2/mydata.txt", data.clone(), &mut cur_label).is_ok());
        assert_eq!(read("/func2/mydata.txt", &mut cur_label).unwrap(), data);
    }
}
