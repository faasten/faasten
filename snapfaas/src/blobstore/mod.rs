use std::io::{Read, Result, Seek, Write};
use std::os::unix::prelude::FileExt;
use std::path::PathBuf;
use std::{ffi::OsString, fs::File, marker::PhantomData};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;


#[derive(Debug)]
pub struct Blobstore<D = Sha256> {
    base_dir: OsString,
    tmp_dir: OsString,
    digest: PhantomData<D>,
}

impl<D> Default for Blobstore<D> {
    fn default() -> Self {
        Blobstore {
            base_dir: OsString::from("blobs"),
            tmp_dir: OsString::from("tmp"),
            digest: PhantomData,
        }
    }
}

impl<D> Blobstore<D> {
    pub const fn new(base_dir: OsString, tmp_dir: OsString) -> Self {
        Blobstore { base_dir, tmp_dir, digest: PhantomData }
    }
}

impl<D: Digest> Blobstore<D> {
    pub fn create(&mut self) -> Result<NewBlob<D>> {
        Ok(NewBlob {
            digest: D::new(),
            file: NamedTempFile::new_in(&self.tmp_dir)?,
        })
    }

    pub fn open(&self, name: String) -> Result<Blob> {
        let blob_path = {
            let (d, n) = name.split_at(2);
            PathBuf::from(&self.base_dir).join(d).join(n)
        };
        Ok(Blob {
            name,
            file: File::open(blob_path)?
        })
    }

    // a hack, see the place that calls vm.launch in worker.rs
    pub fn local_path_string(&self, name: &String) -> Option<String> {
        let (d, n) = name.split_at(2);
        PathBuf::from(&self.base_dir).join(d).join(n).into_os_string().into_string().ok()
    }

    pub fn save(&mut self, new_blob: NewBlob<D>) -> Result<Blob> {
        let name = hex::encode(new_blob.digest.finalize());

        let mut hpath = std::path::PathBuf::new();
        hpath.push(&self.base_dir);
        let (dir, fname) = name.split_at(2);
        hpath.push(dir);
        let _ = std::fs::create_dir_all(hpath.clone());
        hpath.push(fname);
        let file = new_blob.file.persist(hpath)?;
        let mut perms = file.metadata()?.permissions();
        perms.set_readonly(true);
        file.set_permissions(perms)?;
        Ok(Blob {
            name,
            file
        })
    }
}

#[derive(Debug)]
pub struct Blob {
    pub name: String,
    file: File,
}

impl Blob {
    pub fn read_at(&self, buf: &mut [u8], offset: u64) -> Result<usize> {
        self.file.read_at(buf, offset)
    }

}

impl Seek for Blob {
    fn seek(&mut self, pos: std::io::SeekFrom) -> Result<u64> {
        self.file.seek(pos)
    }
}

impl Read for Blob {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.file.read(buf)
    }
}

#[derive(Debug)]
pub struct NewBlob<D: Digest = Sha256> {
    digest: D,
    file: NamedTempFile,
}

impl<D: Digest> Write for NewBlob<D> {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        let n = self.file.write(bytes)?;
        self.digest.update(&bytes[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> Result<()> {
        self.file.flush()
    }
}
