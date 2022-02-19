use super::{Result, Error};
use labeled::dclabel::DCLabel;
use labeled::Label;

pub struct File {
    label: DCLabel,
    data: Vec<u8>,
}

impl File {
    pub fn new(label: DCLabel) -> Self {
        Self { label, data: Vec::new() }
    }

    pub fn label(&self) -> DCLabel {
        self.label.clone()
    }

    pub fn data(&self, cur_label: &DCLabel) -> Result<Vec<u8>> {
        if self.label.can_flow_to(cur_label) {
            Ok(self.data.clone())
        } else {
            Err(Error::Unauthorized)
        }
    }

    pub fn write(&mut self, data: Vec<u8>, cur_label: &DCLabel) -> Result<()> {
        if cur_label.can_flow_to(&self.label) {
            self.data = data;
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    }
}
