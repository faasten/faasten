use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct File {
    data: Vec<u8>,
}

impl File {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn from_vec(buf: Vec<u8>) -> Self {
        serde_json::from_slice(&buf).unwrap()
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }

    pub fn write(&mut self, data: Vec<u8>) {
        self.data = data;
    }
}
