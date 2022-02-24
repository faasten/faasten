use std::path::Path;
use std::collections::HashMap;

use labeled::dclabel::DCLabel;
use labeled::Label;

mod dir;
mod file;

use self::dir::Directory;
use self::file::File;

// types

#[derive(PartialEq, Debug)]
pub enum Error {
    BadPath,
    Unauthorized,
}

type Result<T> = std::result::Result<T, Error>;

pub struct LabeledDirEntry<T> {
    label: DCLabel,
    inner: T
}

impl<T> LabeledDirEntry<T> {
    pub fn unlabel(&self, cur_label: &mut DCLabel) -> &T {
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        &self.inner
    }

    /// First read and raise label if needed, then apply privilege to check if write can happen.
    /// This function always updates `cur_label` to privilege applied.
    pub fn unlabel_mut(&mut self, cur_label: &mut DCLabel, privilege: DCLabel) -> Result<&mut T> {
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        let new_label = cur_label.clone().glb(privilege);
        if new_label.can_flow_to(cur_label) {
            *cur_label = new_label;    
            Ok(&mut self.inner)
        } else {
            Err(Error::Unauthorized)
        }
    }

    fn unlabel_mut_no_write_check(&mut self, cur_label: &mut DCLabel) -> &mut T {
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        &mut self.inner
    }

    pub fn label(&self) -> DCLabel {
        self.label.clone()
    }
}

pub enum DirEntry {
    D(LabeledDirEntry<Directory>),
    F(LabeledDirEntry<File>),
}

impl DirEntry {
    fn label(&self) -> DCLabel {
        match self {
            Self::D(dir) => dir.label(),
            Self::F(file) => file.label(),
        }
    }
}

pub struct LabeledFS {
    dummy_dir: LabeledDirEntry<Directory>,
}

impl LabeledFS {
    // API
    pub fn new() -> Self {
        Self { dummy_dir: LabeledDirEntry { label: DCLabel::bottom(), inner: Directory::new_dummy() } }
    }

    /// read always succeed by raising labels unless the target path is illegal
    pub fn read(&self, path: &str, cur_label: &mut DCLabel) -> Result<Vec<u8>> {
        let path = Path::new(path);
        let dir = path.parent().ok_or(Error::BadPath)?;
        let target = path.file_name().ok_or(Error::BadPath)?.to_str().ok_or(Error::BadPath)?;
        let dir_obj = self.get_dir_obj(dir, cur_label)?;
        if let DirEntry::F(target_obj) = dir_obj.unlabel(cur_label).get(target)? {
            Ok(target_obj.unlabel(cur_label).data())
        } else {
            Err(Error::BadPath)
        }
    }

    /// read always succeed by raising labels unless the target path is illegal
    pub fn list(&self, path: &str, cur_label: &mut DCLabel) -> Result<Vec<String>> {
        if path == "/" {
            let target_obj = self.get_dir_obj(&Path::new(path), cur_label)?;
            Ok(target_obj.unlabel(cur_label).list())
        } else {
            let path = Path::new(path);
            let dir = path.parent().ok_or(Error::BadPath)?;
            let target = path.file_name().ok_or(Error::BadPath)?.to_str().ok_or(Error::BadPath)?;
            let dir_obj = self.get_dir_obj(dir, cur_label)?;
            if let DirEntry::D(target_obj) = dir_obj.unlabel(cur_label).get(target)? {
                Ok(target_obj.unlabel(cur_label).list())
            } else {
                Err(Error::BadPath)
            }
        }
    }

    /// write fails when `cur_label` cannot flow to the target file's label 
    pub fn write(&mut self, path: &str, data: Vec<u8>, cur_label: &mut DCLabel, privilege: DCLabel) -> Result<()> { 
        let path = Path::new(path);
        let dir = path.parent().ok_or(Error::BadPath)?;
        let filename = path.file_name().ok_or(Error::BadPath)?.to_str().ok_or(Error::BadPath)?;
        let dir_obj = self.get_dir_obj_mut(dir, cur_label)?;
        if let DirEntry::F(file_obj) = dir_obj.unlabel_mut_no_write_check(cur_label).get_mut(filename)? {
            Ok(file_obj.unlabel_mut(cur_label, privilege)?.write(data))
        } else {
            Err(Error::BadPath)
        }
    }

    /// create_dir only fails when `cur_label` cannot flow to `label` or target directory's label
    pub fn create_dir(&mut self, dir: &str, name: &str, label: DCLabel, cur_label: &mut DCLabel, privilege: DCLabel) -> Result<()> {
        let dir_obj = self.get_dir_obj_mut(&Path::new(dir), cur_label)?; 
        let new_labeled = LabeledDirEntry {label, inner: Directory::new()};
        dir_obj.unlabel_mut(cur_label, privilege)?.create(name, cur_label, DirEntry::D(new_labeled))
    }

    /// create_file only fails when `cur_label` cannot flow to `label` or target directory's label
    pub fn create_file(&mut self, dir: &str, name: &str, label: DCLabel, cur_label: &mut DCLabel, privilege: DCLabel) -> Result<()> {
        let dir_obj = self.get_dir_obj_mut(&Path::new(dir), cur_label)?; 
        let new_labeled = LabeledDirEntry {label, inner: File::new()};
        dir_obj.unlabel_mut(cur_label, privilege)?.create(name, cur_label, DirEntry::F(new_labeled))
    }

    // helpers

    // return the refenrence to the directory object named by the path
    fn get_dir_obj(&self, path: &Path, cur_label: &mut DCLabel) -> Result<&LabeledDirEntry<Directory>> {
        let mut cur_dir = &self.dummy_dir;
        for component in path.iter() {
            match cur_dir.unlabel(cur_label).get(component.to_str().unwrap())? {
                DirEntry::F(_) => {
                    return Err(Error::BadPath);
                },
                DirEntry::D(dir_obj) => {
                    cur_dir = dir_obj;
                },
            }
        }
        Ok(cur_dir)
    }

    // return the mutable refenrence to the directory object named by the path
    fn get_dir_obj_mut(&mut self, path: &Path, cur_label: &mut DCLabel) -> Result<&mut LabeledDirEntry<Directory>> {
        let mut cur_dir = &mut self.dummy_dir;
        for component in path.iter() {
            match cur_dir.unlabel_mut_no_write_check(cur_label).get_mut(component.to_str().unwrap())? {
                DirEntry::F(_) => {
                    return Err(Error::BadPath);
                },
                DirEntry::D(dir_obj) => {
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

    fn root_privilege() -> DCLabel {
        DCLabel::bottom()
    }

    fn empty_privilege() -> DCLabel {
        DCLabel::top()
    }

    #[test]
    fn test_storage_create_dir_success1() {
        // create `/gh_repo`
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = root_privilege();
        let old_label = cur_label.clone();
        assert!(s.create_dir("/", "gh_repo", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(cur_label, old_label);
    }

    #[test]
    fn test_storage_create_dir_success2() {
        // create `/gh_repo`
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = DCLabel::new([["amit"]], [["yue"]]);
        assert!(s.create_dir("/", "gh_repo", target_label, &mut cur_label, root_privilege()).is_ok());
        assert_eq!(cur_label, root_privilege());
    }

    #[test]
    fn test_storage_create_dir_fail1() {
        // create `/gh_repo/yue`
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let mut cur_label = DCLabel::public();
        let old_label = cur_label.clone();
        assert_eq!(s.create_dir("/gh_repo", "yue", target_label, &mut cur_label, root_privilege()).unwrap_err(), Error::BadPath);
        assert_eq!(cur_label, old_label);
    }

    #[test]
    fn test_storage_create_dir_fail2() {
        // cannot write the parent directory
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = DCLabel::public();
        assert_eq!(s.create_dir("/", "gh_repo", target_label, &mut cur_label, empty_privilege()).unwrap_err(), Error::Unauthorized);
    }

    #[test]
    fn test_storage_create_dir_fail3() {
        // cannot read the parent directory
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new([["yue"]], [["yue"]]);
        let mut cur_label = DCLabel::public();
        assert!(s.create_dir("/", "yue", target_label, &mut cur_label, root_privilege()).is_ok());

        // create a directory of low secrecy in a directory of high secrecy.
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        cur_label = DCLabel::new(true, [["yue"]]);
        assert_eq!(s.create_dir("/yue", "gh_repo", target_label, &mut cur_label, DCLabel::new(false, [["gh_repo"]])).unwrap_err(), Error::Unauthorized);
        assert_eq!(cur_label, DCLabel::new([["yue"]], [["yue"], ["gh_repo"]]));
    }

    #[test]
    fn test_storage_create_dir_fail4() {
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new([["yue"]], [["yue"]]);
        let mut cur_label = DCLabel::public();
        assert!(s.create_dir("/", "yue", target_label, &mut cur_label, root_privilege()).is_ok());

        // target label of low integrity
        let target_label = DCLabel::new([["yue"]], false);
        assert_eq!(s.create_dir("/yue", "gh_repo", target_label, &mut cur_label, empty_privilege()).unwrap_err(), Error::Unauthorized);
    }

    #[test]
    fn test_storage_file_create_read() {
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = root_privilege();
        let old_label = cur_label.clone();
        assert!(s.create_dir("/", "gh_repo", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(old_label, cur_label);

        cur_label = empty_privilege(); // seeing no secrecy and affected by no endorsement

        // create `/gh_repo/yue`
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let privilege = DCLabel::new(true, [["gh_repo"]]);
        assert!(s.create_dir("/gh_repo", "yue", target_label, &mut cur_label, privilege.clone()).is_ok());
        assert_eq!(cur_label, privilege);

        // create `/gh_repo/yue/mydata.txt`
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        assert!(s.create_file("/gh_repo/yue", "mydata.txt", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(cur_label, DCLabel::new([["yue"]], [["gh_repo"]])); // secrecy raised

        let old_label = cur_label.clone();
        assert_eq!(s.read("/gh_repo/yue/mydata.txt", &mut cur_label).unwrap(), Vec::<u8>::new());
        assert_eq!(cur_label, old_label);
    }

    #[test]
    fn test_storage_file_create_write_read() {
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = root_privilege();
        let old_label = cur_label.clone();
        assert!(s.create_dir("/", "gh_repo", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(old_label, cur_label);

        cur_label = empty_privilege(); // seeing no secrecy and affected by no endorsement

        // create `/gh_repo/yue`
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let privilege = DCLabel::new(true, [["gh_repo"]]);
        assert!(s.create_dir("/gh_repo", "yue", target_label, &mut cur_label, privilege.clone()).is_ok());
        assert_eq!(cur_label, privilege);

        // create `/gh_repo/yue/mydata.txt`
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        assert!(s.create_file("/gh_repo/yue", "mydata.txt", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(cur_label, DCLabel::new([["yue"]], [["gh_repo"]])); // secrecy raised

        let text = "test message";
        let data = text.as_bytes().to_vec();
        assert!(s.write("/gh_repo/yue/mydata.txt", data.clone(), &mut cur_label, empty_privilege()).is_ok());

        let old_label = cur_label.clone();
        assert_eq!(s.read("/gh_repo/yue/mydata.txt", &mut cur_label).unwrap(), data);
        assert_eq!(cur_label, old_label);
    }

    #[test]
    fn test_storage_file_create_write_write() {
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = root_privilege();
        let old_label = cur_label.clone();
        assert!(s.create_dir("/", "gh_repo", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(old_label, cur_label);

        cur_label = empty_privilege(); // seeing no secrecy and affected by no endorsement

        // create `/gh_repo/yue`
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let privilege = DCLabel::new(true, [["gh_repo"]]);
        assert!(s.create_dir("/gh_repo", "yue", target_label, &mut cur_label, privilege.clone()).is_ok());
        assert_eq!(cur_label, privilege);

        // create `/gh_repo/yue/mydata.txt`
        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        assert!(s.create_file("/gh_repo/yue", "mydata.txt", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(cur_label, DCLabel::new([["yue"]], [["gh_repo"]])); // secrecy raised

        let text = "test message";
        let data = text.as_bytes().to_vec();
        assert!(s.write("/gh_repo/yue/mydata.txt", data.clone(), &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(s.read("/gh_repo/yue/mydata.txt", &mut cur_label).unwrap(), data);

        let text = "test message test message";
        let data = text.as_bytes().to_vec();
        cur_label = DCLabel::new([["yue"]], true);
        assert!(s.write("/gh_repo/yue/mydata.txt", data.clone(), &mut cur_label, DCLabel::new(true, [["yue"]])).is_ok());
        assert_eq!(s.read("/gh_repo/yue/mydata.txt", &mut cur_label).unwrap(), data);
    }

    #[test]
    fn test_storage_dir_create_list() {
        // create `/gh_repo`
        let mut s = LabeledFS::new();
        let target_label = DCLabel::new(true, [["gh_repo"]]);
        let mut cur_label = DCLabel::public();
        assert!(s.create_dir("/", "gh_repo", target_label, &mut cur_label, root_privilege()).is_ok());
        assert_eq!(cur_label, root_privilege());
        
        cur_label = DCLabel::new(true, [["gh_repo"]]);

        let target_label = DCLabel::new([["yue"]], [["gh_repo"]]);
        let old_label = cur_label.clone();
        assert!(s.create_dir("/gh_repo", "yue", target_label, &mut cur_label, empty_privilege()).is_ok());
        assert_eq!(cur_label, old_label);

        assert_eq!(s.list("/", &mut cur_label).unwrap(), vec![String::from("gh_repo"); 1]);
        assert_eq!(cur_label, old_label);
        assert_eq!(s.list("/gh_repo/yue", &mut cur_label).unwrap(), Vec::<String>::new());
        assert_eq!(cur_label, DCLabel::new([["yue"]], [["gh_repo"]]));
    }
}
