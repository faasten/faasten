use crate::request::Request;
use crate::configs::FunctionConfig;

#[derive(Debug)]
pub struct Vm {
    pub id: usize,
    pub memory: usize,
}

impl Vm {
    pub fn new(id: usize, function_config: &FunctionConfig) -> Vm {
        Vm {
            id: id,
            memory: function_config.memory,
        }
    }

    pub fn process_req(&self, req: Request) -> Result<String, String> {
        return Ok(String::from("success"));
    }

    pub fn shutdown(&self) {
    }
}
