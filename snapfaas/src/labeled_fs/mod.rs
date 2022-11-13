use lazy_static;
use lmdb;
lazy_static::lazy_static! {
    pub static ref DBENV: lmdb::Environment = {
        if !std::path::Path::new("storage").exists() {
            let _ = std::fs::create_dir("storage").unwrap();
        }
        
        lmdb::Environment::new()
            .set_map_size(100 * 1024 * 1024 * 1024)
            .set_max_readers(1024)
            .open(std::path::Path::new("storage"))
            .unwrap()
    };
}

