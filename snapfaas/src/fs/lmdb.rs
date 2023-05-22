use std::time::Instant;

use super::STAT;
use lmdb::{self, Cursor, Transaction, WriteFlags};

pub fn get_dbenv(path: &str) -> lmdb::Environment {
    let path = std::path::Path::new(path);
    if !path.exists() {
        let _ = std::fs::create_dir(path).unwrap();
    }

    lmdb::Environment::new()
        .set_map_size(100 * 1024 * 1024 * 1024)
        .set_max_readers(1024)
        .open(path)
        .unwrap()
}

impl super::BackingStore for lmdb::Environment {
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

    fn del(&self, key: &[u8]) {
        STAT.with(|_stat| {
            let db = self.open_db(None).unwrap();
            let mut txn = self.begin_rw_txn().unwrap();
            let _ = txn.del(db, &key, None);
            txn.commit().unwrap();
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
