use serde::{Deserialize, Serialize};
use labeled::dclabel::DCLabel;
use labeled::Label;

use super::{Result, Error};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum DirEntry {
    D,
    F,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LabeledDirEntry {
    label: DCLabel,
    entry_type: DirEntry,
    uid: u64,
}

impl LabeledDirEntry {
    pub fn new(label: DCLabel, entry_type: DirEntry) -> Self {
        Self { label, entry_type, uid: super::get_next_uid() }
    }

    pub fn root() -> Self {
        Self { label: DCLabel::bottom(), entry_type: DirEntry::D, uid: 0u64 }
    }

    /// raise label if necessary, and return the uid of the object
    pub fn unlabel(&self, cur_label: &mut DCLabel) -> &Self {
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        &self
    }

    /// First read and raise label if needed, then apply privilege to check if write can happen.
    /// This function always updates `cur_label` to privilege applied.
    pub fn unlabel_write_check(&self, cur_label: &mut DCLabel, privilege: DCLabel) -> Result<&Self> {
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        let new_label = cur_label.clone().glb(privilege);
        if new_label.can_flow_to(cur_label) {
            *cur_label = new_label;    
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
