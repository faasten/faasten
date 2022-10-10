use serde::{Deserialize, Serialize};
use labeled::dclabel::DCLabel;
use labeled::Label;

use super::{Result, Error};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum DirEntry {
    D,
    F,
    Gate,
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
        Self { label: DCLabel::bottom(), entry_type: DirEntry::D, uid: 0u64 }
    }

    // This function does only read check and returns a reference to the entry object
    pub fn unlabel(&self, cur_label: &mut DCLabel) -> &Self {
        // raise label when cur_label cannot flow to the entry label
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        &self
    }

    // This function does read and write checks and returns a reference to the entry
    // object if write is allowed and Error::Unauthorized otherwise.
    pub fn unlabel_write_check(&self, cur_label: &mut DCLabel) -> Result<&Self> {
        // First read and raise label if needed then check if the writer can write
        // write implies read
        if !self.label.can_flow_to(cur_label) {
            *cur_label = self.label.clone().lub(cur_label.clone());
        }
        if cur_label.can_flow_to(&self.label) {
            Ok(&self)
        } else {
            Err(Error::Unauthorized)
        }
    }

    // This function does invoke check and returns the secrecy component of the entry
    // label as the extra privilege.
    pub fn unlabel_invoke_check(&self, cur_label: &mut DCLabel) -> Result<&Self> {
        // Check if cur_label's integrity implies the gate's integrity.
        let mut l = DCLabel::bottom();
        l.integrity = cur_label.integrity.clone();
        if l.can_flow_to(&self.label){
            return Ok(&self);
        }
        Err(Error::Unauthorized)
    }

    pub fn uid(&self) -> u64 {
        self.uid
    }

    pub fn entry_type(&self) -> DirEntry {
        self.entry_type
    }

    pub fn label(&self) -> &DCLabel {
        &self.label
    }
}
