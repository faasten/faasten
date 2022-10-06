use serde::{Deserialize, Serialize};
use labeled::dclabel::DCLabel;
use labeled::Label;

use super::{Result, Error};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum DirEntry {
    D,
    F,
    FacetedD,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LabeledDirEntry {
    label: DCLabel,
    entry_type: DirEntry,
    uid: u64,
}

impl LabeledDirEntry {
    pub fn new(label: DCLabel, entry_type: DirEntry, uid: u64) -> Self {
        Self { label, entry_type, uid }
    }

    pub fn root() -> Self {
        Self { label: DCLabel::bottom(), entry_type: DirEntry::FacetedD, uid: 0u64 }
    }

    /// raise label if necessary, and return the uid of the object
    pub fn unlabel(&self, cur_label: &mut DCLabel) -> &Self {
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        &self
    }

    /// First read and raise label if needed then check if the writer can write
    pub fn unlabel_write_check(&self, cur_label: &mut DCLabel) -> Result<&Self> {
        // Enforcing that write implies read prevents low writers from writing high files
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        if cur_label.can_flow_to(&self.label) {
            Ok(&self)
        } else {
            Err(Error::Unauthorized)
        }
    }

    pub fn uid(&self) -> u64 {
        self.uid
    }

    pub fn entry_type(&self) -> DirEntry {
        self.entry_type
    }
}
