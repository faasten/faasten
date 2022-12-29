use std::net::{TcpStream, SocketAddr};
use std::thread;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};

use super::Error;
use super::message;
use super::message::{Request, Response};
use super::message::request::Kind as ReqKind;

/// RPC calls
#[derive(Debug, Clone)]
pub struct Scheduler {
    sa: SocketAddr,
}

impl Scheduler {
    pub fn new(sa: SocketAddr) -> Self {
        Scheduler { sa }
    }

    fn connect(&self) -> Result<TcpStream, Error> {
        TcpStream::connect(&self.sa).map_err(|e| Error::StreamConnect(e))
    }

    /// This method is for workers to retrieve a HTTP request, and
    /// it is supposed to block if there's no further HTTP requests
    pub fn get(&self) -> Result<Response, Error> {
        let mut stream = self.connect()?;
        let id = {
            // avoid using unstable #![feature(thread_id_value)]
            let mut hasher = DefaultHasher::new();
            thread::current().id().hash(&mut hasher);
            hasher.finish()
        };
        let req = Request {
            kind: Some(ReqKind::GetJob(message::GetJob { id })),
        };
        message::write(&mut stream, req)?;
        let response = message::read_response(&mut stream)?;
        Ok(response)
    }

    /// This method is for workers to return the result of a HTTP request
    pub fn finish(
        &self, id: String, result: Vec<u8>
    ) -> Result<Response, Error> {
        let mut stream = self.connect()?;
        let req = Request {
            kind: Some(ReqKind::FinishJob(message::FinishJob { id, result })),
        };
        message::write(&mut stream, req)?;
        let response = message::read_response(&mut stream)?;
        Ok(response)
    }

    /// This method is for workers to invoke a function
    pub fn invoke(&self, invoke: Vec<u8>) -> Result<(), Error> {
        let mut stream = self.connect()?;
        let req = Request {
            kind: Some(ReqKind::Invoke(message::Invoke {invoke}))
        };
        message::write(&mut stream, req)?;
        let _ = message::read_response(&mut stream)?;
        Ok(())
    }

    /// This method is to shutdown all workers (for debug)
    pub fn shutdown_all(&self) -> Result<(), Error> {
        let mut stream = self.connect()?;
        let req = Request {
            kind: Some(ReqKind::ShutdownAll(message::ShutdownAll {})),
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
    ) -> Result<(), Error> {
        let mut stream = self.connect()?;
        let info = serde_json::to_vec(&info).unwrap();
        let req = Request {
            kind: Some(ReqKind::UpdateResource(message::UpdateResource { info })),
        };
        message::write(&mut stream, req)?;
        let _ = message::read_response(&mut stream)?;
        Ok(())
    }

    /// TODO This method is for local resrouce managers to drop itself
    pub fn drop_resource(&self) -> Result<(), Error> {
        let mut stream = self.connect()?;
        let req = Request {
            kind: Some(ReqKind::DropResource(message::DropResource {})),
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
