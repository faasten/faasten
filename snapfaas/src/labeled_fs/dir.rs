use super::{EntryT, Result, Error};
use labeled::dclabel::DCLabel;
use labeled::Label;
use std::collections::HashMap;

pub struct Directory {
    label: DCLabel,
    mappings: HashMap<String, EntryT>,
}

impl Directory {
    /// The `/` root directory is publicly observable and only writable by the system admin.
    pub fn new_root() -> Self {
        Self { label: DCLabel::bottom(),  mappings: HashMap::new() }
    }

    /// Create a new labeled empty directory.
    pub fn new(label: DCLabel) -> Self {
        Self { label, mappings: HashMap::new() }
    }

    pub fn get(&self, name: &str, cur_label: &DCLabel) -> Result<&EntryT> {
        if self.label.can_flow_to(cur_label) {
            self.mappings.get(name).ok_or(Error::BadPath)
        } else {
            Err(Error::Unauthorized)
        }
    }

    pub fn get_mut(&mut self, name: &str, cur_label: &DCLabel) -> Result<&mut EntryT> {
        if self.label.can_flow_to(cur_label) {
            self.mappings.get_mut(name).ok_or(Error::BadPath)
        } else {
            Err(Error::Unauthorized)
        }
    }

    pub fn create(&mut self, name: &str, cur_label: &DCLabel, new_entry: EntryT) -> Result<()> {
        if self.label.can_flow_to(cur_label) && cur_label.can_flow_to(&self.label) && cur_label.can_flow_to(&new_entry.label()) {
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

    pub fn label(&self) -> DCLabel {
        self.label.clone()
    }

    pub fn list(&self, cur_label: &DCLabel) -> Result<Vec<String>> {
        if self.label.can_flow_to(cur_label) {
            Ok(self.mappings.keys().cloned().collect())
        } else {
            Err(Error::Unauthorized)
        }
    }
}
