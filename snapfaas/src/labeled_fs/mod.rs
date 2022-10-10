use std::path::Path;

use rand::{self, RngCore};
use lazy_static;
use lmdb;
use lmdb::{Transaction, WriteFlags};
use labeled::dclabel::{Component, DCLabel};
use labeled::Label;

mod dir;
mod file;
mod direntry;
pub mod utils;

use self::direntry::{LabeledDirEntry, DirEntry};
use self::dir::Directory;
use self::file::File;

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
        let _ = put_val_db_no_overwrite(root_uid, Directory::new().to_vec(), &mut txn, default_db);
        txn.commit().unwrap();

        dbenv
    };
}

#[derive(PartialEq, Debug)]
pub enum Error {
    BadPath,
    Unauthorized,
    BadTargetLabel,
}

type Result<T> = std::result::Result<T, Error>;

//////////////
//   APIs   //
//////////////
/// read always succeeds by raising labels unless the target path is illegal
pub fn read(path: &str, cur_label: &mut DCLabel) -> Result<Vec<u8>> {
    let db = DBENV.open_db(None).unwrap();
    let txn = DBENV.begin_ro_txn().unwrap();
    let res = get_direntry(path, cur_label, &txn, db).and_then(|labeled| -> Result<Vec<u8>> {
        let entry = labeled.unlabel(cur_label);
        match entry.entry_type() {
            DirEntry::F => {
                let file = get_val_db(entry.uid(), &txn, db).map(File::from_vec).unwrap();
                Ok(file.data())
            },
            _ => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/// read always succeed by raising labels unless the target path is illegal
pub fn list(path: &str, cur_label: &mut DCLabel) -> Result<Vec<String>> {
    let db = DBENV.open_db(None).unwrap();
    let txn = DBENV.begin_ro_txn().unwrap();
    let res = get_direntry(path, cur_label, &txn, db).and_then(|labeled| -> Result<Vec<String>> {
        let entry = labeled.unlabel(cur_label);
        match entry.entry_type() {
            DirEntry::D => {
                let dir = get_val_db(entry.uid(), &txn, db).map(Directory::from_vec).unwrap();
                Ok(dir.list())
            },
            _ => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/// create_dir only fails when `cur_label` cannot flow to `label` or target directory's label
pub fn create_dir(base_dir: &str, name: &str, label: DCLabel, cur_label: &mut DCLabel) -> Result<()> {
    create_common(base_dir, name, label, cur_label, Directory::new().to_vec(), DirEntry::D)
}

/// create_file only fails when `cur_label` cannot flow to `label` or target directory's label
pub fn create_file(base_dir: &str, name: &str, label: DCLabel, cur_label: &mut DCLabel) -> Result<()> {
    create_common(base_dir, name, label, cur_label, File::new().to_vec(), DirEntry::F)
}

/// write fails when `cur_label` cannot flow to the target file's label
pub fn write(path: &str, data: Vec<u8>, cur_label: &mut DCLabel) -> Result<()> {
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
            _ => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/// A gate is a entry type supported by the file system. A gate allows the invoker to invoke
/// the function the gate points to with extra privilege. A gate path should end with the
/// function name it points to. A gate label's secrecy should be the extra privlege.
/// A gate label's integrity controls who can invoke the gate. Only when the invoker's
/// integrity implies the gate's integrity is the invocation allowed.
pub fn create_gate(base_dir: &str, function: &str, label: DCLabel, cur_label: &mut DCLabel, privilege: Component) -> Result<()> {
    // privilege must imply label.secrecy, i.e. label must flow to <privilege, true>
    if !label.can_flow_to(&DCLabel::new(privilege, true)) {
        return Err(Error::BadTargetLabel);
    }
    // Vec::new() is a dummy argument because there is no gate object but only gate entry.
    create_common(base_dir, function, label, cur_label, Vec::new(), DirEntry::Gate)
}

/// See create_gate.
pub fn invoke_gate(path: &str, cur_label: &mut DCLabel) -> Result<Component> {
    let db = DBENV.open_db(None).unwrap();
    let txn = DBENV.begin_ro_txn().unwrap();
    let res = get_direntry(path, cur_label, &txn, db).and_then(|labeled| -> Result<Component> {
        let entry = labeled.unlabel_invoke_check(cur_label)?;
        match entry.entry_type() {
            DirEntry::Gate => {
                Ok(entry.label().secrecy.clone())
            },
            _ => Err(Error::BadPath),
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

// return the labeled direntry named by the path
fn get_direntry<T>(path: &str, cur_label: &mut DCLabel, txn: &T, db: lmdb::Database) -> Result<LabeledDirEntry>
where T: Transaction
{
    let path = Path::new(path);
    let mut labeled = LabeledDirEntry::root();
    let mut it = path.iter();
    let _ = it.next();
    for component in it {
        let entry = labeled.unlabel(cur_label);
        match entry.entry_type() {
            DirEntry::D => {
                let cur_dir = get_val_db(entry.uid(), txn, db).map(Directory::from_vec).unwrap();
                labeled = cur_dir.get(component.to_str().unwrap())?.clone();
            },
            _ => {
                return Err(Error::BadPath);
            },
        }
    }
    Ok(labeled)
}

fn create_common(
    base_dir: &str,
    name: &str,
    label: DCLabel,
    cur_label: &mut DCLabel,
    obj_vec: Vec<u8>,
    entry_type: DirEntry,
) -> Result<()> {
    let db = DBENV.open_db(None).unwrap();
    let mut txn = DBENV.begin_rw_txn().unwrap();
    let res = get_direntry(base_dir, cur_label, &txn, db).and_then(|labeled| -> Result<()> {
        let entry = labeled.unlabel_write_check(cur_label)?;
        match entry.entry_type() {
            DirEntry::D => {
                let mut dir = get_val_db(entry.uid(), &txn, db).map(Directory::from_vec).unwrap();
                dir.create(name, cur_label, obj_vec, entry_type, label, &mut txn, db)?;
                let _ = put_val_db(entry.uid(), dir.to_vec(), &mut txn, db);
                Ok(())
            },
            _ => Err(Error::BadPath),
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
