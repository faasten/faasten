use super::{LabeledDirEntry, DirEntry, Result, Error};
use labeled::dclabel::DCLabel;
use labeled::Label;
use std::collections::HashMap;

pub struct Directory {
    mappings: HashMap<String, DirEntry>,
}

impl Directory {
    /// Create a dummy directory that contains only the root directory mapping
    pub fn new_dummy() -> Self {
        let labeled_root = LabeledDirEntry { label: DCLabel::bottom(), inner: Directory::new() };
        let mut mappings = HashMap::new();
        assert!(mappings.insert(String::from("/"), DirEntry::D(labeled_root)).is_none());
        Directory { mappings }
    }

    /// Create a new labeled empty directory.
    pub fn new() -> Self {
        Self { mappings: HashMap::new() }
    }

    pub fn get(&self, name: &str) -> Result<&DirEntry> {
        self.mappings.get(name).ok_or(Error::BadPath)
    }

    pub fn get_mut(&mut self, name: &str) -> Result<&mut DirEntry> {
        self.mappings.get_mut(name).ok_or(Error::BadPath)
    }

    pub fn create(&mut self, name: &str, cur_label: &DCLabel, new_entry: DirEntry) -> Result<()> {
        if cur_label.can_flow_to(&new_entry.label()) {
            if let Some(_) = self.mappings.get(name) {
                 Err(Error::BadPath)
            } else {
                 assert!(self.mappings.insert(name.to_string(), new_entry).is_none());
                 Ok(())
            }
        } else {
            Err(Error::Unauthorized)
        }
    }

    pub fn list(&self) -> Vec<String> {
        self.mappings.keys().cloned().collect()
    }
}
