use lmdb::{self, Transaction, WriteFlags};

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
        let db = self.open_db(None).ok()?;
        let txn = self.begin_ro_txn().ok()?;
        let res = txn.get(db, &key).ok().map(Into::<Vec<u8>>::into);
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

    fn cas(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        value: &[u8],
    ) -> Result<(), Option<Vec<u8>>> {
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

    fn del(&self, key: &[u8]) {
        let db = self.open_db(None).unwrap();
        let mut txn = self.begin_rw_txn().unwrap();
        let _ = txn.del(db, &key, None);
        txn.commit().unwrap();
    }
}
