use std::net::{TcpStream, SocketAddr};
use std::error::Error;
use std::thread;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};

use super::message;
use super::message::{request, Request, Response};
// use super::resource_manager::LocalResourceManagerInfo;


/// RPC calls
#[derive(Debug, Clone)]
pub struct Scheduler {
    sa: SocketAddr,
}

impl Scheduler {
    pub fn new(sa: SocketAddr) -> Self {
        Scheduler { sa }
    }

    fn connect(&self) -> Result<TcpStream, Box<dyn Error>> {
        let stream = TcpStream::connect(&self.sa)?;
        Ok(stream)
    }

    /// This method is for workers to retrieve a HTTP request, and
    /// it is supposed to block if there's no further HTTP requests
    pub fn recv(&self) -> Result<Response, Box<dyn Error>> {
        let mut stream = self.connect()?;
        let id = {
            // avoid using unstable #![feature(thread_id_value)]
            let mut hasher = DefaultHasher::new();
            thread::current().id().hash(&mut hasher);
            hasher.finish()
        };
        let req = Request {
            kind: Some(request::Kind::Begin(id)),
        };
        message::write(&mut stream, req)?;
        let response = message::read_response(&mut stream)?;
        Ok(response)
    }

    /// This method is for workers to return the result of a HTTP request
    pub fn send(
        &self, result: Vec<u8>
    ) -> Result<Response, Box<dyn Error>> {
        let mut stream = self.connect()?;
        let req = Request {
            kind: Some(request::Kind::Finish(result)),
        };
        message::write(&mut stream, req)?;
        let response = message::read_response(&mut stream)?;
        Ok(response)
    }

    /// This method is for workers to invoke a function
    pub fn invoke(&self, request: Vec<u8>) -> Result<(), Box<dyn Error>> {
        let mut stream = self.connect()?;
        let req = Request {
            kind: Some(request::Kind::Invoke(request)),
        };
        message::write(&mut stream, req)?;
        let _ = message::read_response(&mut stream)?;
        Ok(())
    }


    /// This method is to shutdown all workers (for debug)
    pub fn shutdown_all(&self) -> Result<(), Box<dyn Error>> {
        let mut stream = self.connect()?;
        let buf = "".as_bytes().to_vec();
        let req = Request {
            kind: Some(request::Kind::ShutdownAll(buf)),
        };
        message::write(&mut stream, req)?;
        let _ = message::read_response(&mut stream)?;
        Ok(())
    }

    /// This method is for local resource managers to update it's
    /// resource status, such as number of cached VMs per function
    pub fn update_resource(
        &self,
        info: ResourceInfo
    ) -> Result<(), Box<dyn Error>> {
        let mut stream = self.connect()?;
        let buf = serde_json::to_vec(&info).unwrap();
        let req = Request {
            kind: Some(request::Kind::UpdateResource(buf)),
        };
        message::write(&mut stream, req)?;
        let _ = message::read_response(&mut stream)?;
        Ok(())
    }

    /// TODO This method is for local resrouce managers to drop itself
    pub fn drop_resource(&self) -> Result<(), Box<dyn Error>> {
        let mut stream = self.connect()?;
        let buf = "".as_bytes().to_vec();
        let req = Request {
            kind: Some(request::Kind::DropResource(buf)),
        };
        message::write(&mut stream, req)?;
        let _ = message::read_response(&mut stream)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub stats: HashMap<String, usize>,
    pub total_mem: usize,
    pub free_mem: usize,
}
