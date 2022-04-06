pub struct File {
    data: Vec<u8>,
}

impl File {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }

    pub fn write(&mut self, data: Vec<u8>) {
        self.data = data;
    }
}
