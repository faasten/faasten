use crate::request::Request;

#[derive(Debug)]
pub struct Vm {
    pub id: usize,
}

impl Vm {
    pub fn new(id: usize) -> Vm {
        Vm {
            id: id,
        }
    }

    pub fn send_req(&self, req: Request) -> Result<String, String> {
        return Ok(String::from("success"));
    }
}
