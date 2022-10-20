use labeled::dclabel::DCLabel;
use labeled::Label;
use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use super::{LabeledDirEntry, DirEntry, Result, Error};

#[derive(Serialize, Deserialize)]
pub struct Directory {
    mappings: BTreeMap<String, LabeledDirEntry>,
}

impl Directory {
    /// Create a new labeled empty directory.
    pub fn new() -> Self {
        Self { mappings: BTreeMap::new() }
    }

    pub fn from_vec(buf: Vec<u8>) -> Self {
        serde_json::from_slice(&buf).unwrap()
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub fn get(&self, name: &str) -> Result<&LabeledDirEntry> {
        self.mappings.get(name).ok_or(Error::BadPath)
    }

    pub fn create(
        &mut self,
        name: &str,
        cur_label: &DCLabel,
        entry_type: DirEntry,
        label: Option<DCLabel>,
        uid: u64
    ) -> Result<u64> {
        let label = label.unwrap_or(cur_label.clone());
        if cur_label.can_flow_to(&label) {
            if let Some(_) = self.mappings.get(name) {
                Err(Error::BadPath)
            } else {
                let new_entry = LabeledDirEntry::new(label, entry_type, uid);
                let uid = new_entry.uid();
                let _ = self.mappings.insert(name.to_string(), new_entry);
                Ok(uid)
            }
        } else {
            Err(Error::BadTargetLabel)
        }
    }

    pub fn list(&self) -> Vec<String> {
        self.mappings.keys().cloned().collect()
    }
}
