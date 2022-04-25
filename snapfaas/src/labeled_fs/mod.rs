use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use lazy_static;
use lmdb;
use lmdb::{Transaction, WriteFlags};
use labeled::dclabel::DCLabel;

mod dir;
mod file;
mod direntry;
pub mod utils;

use self::direntry::{LabeledDirEntry, DirEntry};
use self::dir::Directory;
use self::file::File;

lazy_static::lazy_static! {
    static ref NEXT_UID: AtomicU64 = AtomicU64::new(0);
    pub static ref DBENV: lmdb::Environment = {
        let dbenv = lmdb::Environment::new()
            .set_map_size(100 * 1024 * 1024 * 1024)
            .open(std::path::Path::new("storage"))
            .unwrap();

        let default_db = dbenv.open_db(None).unwrap();
        let mut txn = dbenv.begin_rw_txn().unwrap();
        let root_uid = get_next_uid();
        if let Err(lmdb::Error::NotFound) = get_val_db(root_uid, &txn, default_db) {
            put_val_db(root_uid, Directory::new().to_vec(), &mut txn, default_db);
        }
        txn.commit().unwrap();

        dbenv
    };
}

#[derive(PartialEq, Debug)]
pub enum Error {
    BadPath,
    Unauthorized,
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
            DirEntry::D => Err(Error::BadPath),
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
            DirEntry::F => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/// create_dir only fails when `cur_label` cannot flow to `label` or target directory's label
pub fn create_dir(dir: &str, name: &str, label: DCLabel, cur_label: &mut DCLabel) -> Result<()> {
    let db = DBENV.open_db(None).unwrap();
    let mut txn = DBENV.begin_rw_txn().unwrap();
    let res = get_direntry(dir, cur_label, &txn, db).and_then(|labeled| -> Result<()> {
        let entry = labeled.unlabel_write_check(cur_label)?;
        match entry.entry_type() {
            DirEntry::D => {
                let mut dir = get_val_db(entry.uid(), &txn, db).map(Directory::from_vec).unwrap();
                let uid = dir.create(name, cur_label, DirEntry::D, label)?;
                put_val_db(uid, Directory::new().to_vec(), &mut txn, db);
                put_val_db(entry.uid(), dir.to_vec(), &mut txn, db);
                Ok(())
            },
            DirEntry::F => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/// create_file only fails when `cur_label` cannot flow to `label` or target directory's label
pub fn create_file(dir: &str, name: &str, label: DCLabel, cur_label: &mut DCLabel) -> Result<()> {
    let db = DBENV.open_db(None).unwrap();
    let mut txn = DBENV.begin_rw_txn().unwrap();
    let res = get_direntry(dir, cur_label, &txn, db).and_then(|labeled| -> Result<()> {
        let entry = labeled.unlabel_write_check(cur_label)?;
        match entry.entry_type() {
            DirEntry::D => {
                let mut dir = get_val_db(entry.uid(), &txn, db).map(Directory::from_vec).unwrap();
                let uid = dir.create(name, cur_label, DirEntry::F, label)?;
                put_val_db(uid, File::new().to_vec(), &mut txn, db);
                put_val_db(entry.uid(), dir.to_vec(), &mut txn, db);
                Ok(())
            },
            DirEntry::F => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
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
                put_val_db(entry.uid(), file.to_vec(), &mut txn, db);
                Ok(())
            }
            DirEntry::D => Err(Error::BadPath),
        }
    });
    txn.commit().unwrap();
    res
}

/////////////
// helpers //
/////////////
// return a uid in big-endian bytes
fn get_next_uid() -> u64 {
    NEXT_UID.fetch_add(1, Ordering::Relaxed)
}

fn get_val_db<T>(uid: u64, txn: &T, db: lmdb::Database) -> std::result::Result<Vec<u8>, lmdb::Error>
where T: Transaction {
    txn.get(db, &uid.to_be_bytes()).map(Vec::from)
}

fn put_val_db(uid: u64, val: Vec<u8>, txn: &mut lmdb::RwTransaction, db: lmdb::Database) {
    let _ = txn.put(db, &uid.to_be_bytes(), &val, WriteFlags::empty());
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
            DirEntry::F => {
                return Err(Error::BadPath);
            },
            DirEntry::D => {
                let cur_dir = get_val_db(entry.uid(), txn, db).map(Directory::from_vec).unwrap();
                labeled = cur_dir.get(component.to_str().unwrap())?.clone();
            },
        }
    }
    Ok(labeled)
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
        // create `/gh_repo/yue`
        let mut cur_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        assert!(create_dir("/gh_repo", "yue", target_label, &mut cur_label).is_ok());

        // create `/gh_repo/yue/mydata.txt`
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        assert!(create_file("/gh_repo/yue", "mydata.txt", target_label, &mut cur_label).is_ok());
        assert_eq!(read("/gh_repo/yue/mydata.txt", &mut cur_label).unwrap(), Vec::<u8>::new());
    
        // write read
        let text = "test message";
        let data = text.as_bytes().to_vec();
        assert!(write("/gh_repo/yue/mydata.txt", data.clone(), &mut cur_label).is_ok());
        assert_eq!(read("/gh_repo/yue/mydata.txt", &mut cur_label).unwrap(), data);

        // overwrite read
        let text = "test message test message";
        let data = text.as_bytes().to_vec();
        assert!(write("/gh_repo/yue/mydata.txt", data.clone(), &mut cur_label).is_ok());
        assert_eq!(read("/gh_repo/yue/mydata.txt", &mut cur_label).unwrap(), data);
    }
}
