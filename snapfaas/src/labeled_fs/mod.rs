use labeled::Label;
use labeled::dclabel::DCLabel;

mod dir;
mod file;

use self::dir::Directory;
use self::file::File;

// types

pub enum Error {
    BadPath,
    Unauthorized,
}

type Result<T> = std::result::Result<T, Error>;

pub enum EntryT {
    D(Directory),
    F(File),
}

impl EntryT {
    fn label(&self) -> DCLabel {
        match self {
            Self::D(dir) => dir.label(),
            Self::F(file) => file.label(),
        }
    }
}

pub struct LabeledStorage {
    root: Directory,
}

impl LabeledStorage {
    // API
    pub fn new() -> Self {
        Self { root: Directory::new_root() }
    }

    pub fn create_dir(&mut self, dir: &str, name: &str, label: DCLabel, cur_label: &DCLabel) -> Result<()> {
        let dir_obj = self.get_dir_obj_mut(dir, cur_label)?; 
        dir_obj.create(name, cur_label, EntryT::D(Directory::new(label)))
    }

    pub fn create_file(&mut self, dir: &str, name: &str, label: DCLabel, cur_label: &DCLabel) -> Result<()> {
        let dir_obj = self.get_dir_obj_mut(dir, cur_label)?; 
        dir_obj.create(name, cur_label, EntryT::F(File::new(label)))
    }

    pub fn write(&mut self, path: &str, data: Vec<u8>, cur_label: &DCLabel) -> Result<()> { 
        let (dir, filename) = path.rsplit_once("/").ok_or(Error::BadPath)?;
        let dir_obj = self.get_dir_obj_mut(dir, cur_label)?;
        if let EntryT::F(file_obj) = dir_obj.get_mut(filename, cur_label)? {
            file_obj.write(data, cur_label)
        } else {
            Err(Error::BadPath)
        }
    }

    pub fn read(&self, path: String, cur_label: &DCLabel) -> Result<(DCLabel, Vec<u8>)> {
        let (dir, target) = path.rsplit_once("/").ok_or(Error::BadPath)?;
        let dir_obj = self.get_dir_obj(dir, cur_label)?;
        if let EntryT::F(target_obj) = dir_obj.get(target, cur_label)? {
            Ok((target_obj.label(), target_obj.data(cur_label)?))
        } else {
            Err(Error::BadPath)
        }
    }

    pub fn list(&self, path: String, cur_label: &DCLabel) -> Result<(DCLabel, Vec<String>)> {
        let (dir, target) = path.rsplit_once("/").ok_or(Error::BadPath)?;
        let dir_obj = self.get_dir_obj(dir, cur_label)?;
        if let EntryT::D(target_obj) = dir_obj.get(target, cur_label)? {
            Ok((dir_obj.label(), target_obj.list(cur_label)?))
        } else {
            Err(Error::BadPath)
        }
    }

    // helpers

    // return the refenrence to the directory object named by the path
    fn get_dir_obj(&self, path: &str, cur_label: &DCLabel) -> Result<&Directory> {
        let components: Vec<&str> = path.split("/").collect();
        let mut cur_dir = &self.root;
        for d in components {
            match cur_dir.get(d, cur_label)? {
                EntryT::F(_) => {
                    return Err(Error::BadPath);
                },
                EntryT::D(dir_obj) => {
                    cur_dir = &dir_obj;
                },
            }
        }
        Ok(cur_dir)
    }

    // return the mutable refenrence to the directory object named by the path
    fn get_dir_obj_mut(&mut self, path: &str, cur_label: &DCLabel) -> Result<&mut Directory> {
        let components: Vec<&str> = path.split("/").collect();
        let mut cur_dir = &mut self.root;
        for d in components {
            match cur_dir.get_mut(d, cur_label)? {
                EntryT::F(_) => {
                    return Err(Error::BadPath);
                },
                EntryT::D(dir_obj) => {
                    cur_dir = dir_obj;
                },
            }
        }
        Ok(cur_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_create_dir_success() {
        let mut s = LabeledStorage::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let root_label: DCLabel = DCLabel::bottom();
        assert!(s.create_dir("/", "gh_repo", target_label, &root_label).is_ok());
    }

    #[test]
    fn test_storage_create_dir_fail1() {
        let mut s = LabeledStorage::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let cur_label = DCLabel::new([["amit"]], true);
        assert!(s.create_dir("/", "gh_repo", target_label, &cur_label).is_err());
    }

    #[test]
    fn test_storage_create_dir_fail2() {
        let mut s = LabeledStorage::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let cur_label = DCLabel::new([["amit"]], true);
        assert!(s.create_dir("/", "gh_repo", target_label, &cur_label).is_err());
    }

    #[test]
    fn test_storage_create_file_success() {
    }

    #[test]
    fn test_storage_create_file_fail() {
    }
}
