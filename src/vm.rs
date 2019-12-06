use crate::request::Request;

#[derive(Debug)]
pub struct Vm {
    pub id: usize,
    pub memory: usize,
}

impl Vm {
    pub fn new(id: usize) -> Vm {
        Vm {
            id: id,
            memory: 0,
        }
    }

    pub fn process_req(&self, req: Request) -> Result<String, String> {
        return Ok(String::from("success"));
    }

    pub fn shutdown(&self) {
    }
}
