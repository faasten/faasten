pub mod gateway;
pub mod resource_manager;
pub mod message;

use std::net::TcpStream;
use std::error::Error;
use std::thread;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use prost::Message;

use crate::request::Request as HTTPRequest;
use crate::resource_manager::ResourceManager
    as LocalResourceManger;
// use self::resource_manager::ResourceManager;
use self::message::{
    request, response,
    Request, Response,
};
use self::resource_manager::LocalResourceManagerInfo;

pub fn schedule(
    request: HTTPRequest, resman: self::gateway::Manager,
) -> Result<(), Box<dyn Error>> {

    use log::debug;
    debug!("schedule start");

    let mut resman = resman.lock().unwrap();
    let function = &request.function;

    // FIXME no idle worker found
    let mut stream = resman
        .find_idle(function)
        .map(|w| w.stream)
        .unwrap_or_else(|| {
            panic!("no idle worker found")
        });

    // forward http request
    let buf = request.to_vec();
    let res = Response {
        kind: Some(response::Kind::Process(buf)),
    }.encode_to_vec();
    let _ = message::send_to(&mut stream, res)?;

    // receive response
    let req = message::recv_from(&mut stream)
        .and_then(|b| {
            let req = Request::decode(&b[..])?;
            Ok(req)
        });
    let res = Response {
        kind: None
    }.encode_to_vec();
    let _ = message::send_to(&mut stream, res);

    debug!("sched recv from worker {:?}", req);
    debug!("schedule end");

    Ok(())
}

// RPC calls
pub struct Scheduler {
    stream: TcpStream,
}

impl Scheduler {
    pub fn connect(addr: &str) -> Self {
        let stream = TcpStream::connect(addr).unwrap();
        Scheduler { stream }
    }

    // Auxiliary function that sends a RPC request
    fn send(&mut self, req: Vec<u8>) -> Result<(), Box<dyn Error>> {
        message::send_to(&mut self.stream, req)
    }

    // Auxiliary function that reads a RPC response
    fn read(&mut self) -> Result<Response, Box<dyn Error>> {
        let buf = message::recv_from(&mut self.stream)?;
        let req = Response::decode(&buf[..])?;
        Ok(req)
    }

    // This method is for workers to retrieve a HTTP request, and
    // it is supposed to block if there's no further HTTP requests
    pub fn recv(&mut self) -> Result<Response, Box<dyn Error>> {
        // avoid using unstable #![feature(thread_id_value)]
        let id = {
            let mut hasher = DefaultHasher::new();
            thread::current().id().hash(&mut hasher);
            hasher.finish()
        };
        let req = Request {
            kind: Some(request::Kind::Begin(id)),
        }.encode_to_vec();
        self.send(req)?;
        let response = self.read()?;
        Ok(response)
    }

    // This method is for workers to return the result of a HTTP request
    pub fn retn(
        &mut self, result: Vec<u8>
    ) -> Result<Response, Box<dyn Error>> {
        let req = Request {
            kind: Some(request::Kind::Finish(result)),
        }.encode_to_vec();
        self.send(req)?;
        let response = self.read()?;
        Ok(response)
    }

    pub fn shutdown_all(&mut self) -> Result<(), Box<dyn Error>> {
        let buf = "".as_bytes().to_vec();
        let req = Request {
            kind: Some(request::Kind::ShutdownAll(buf)),
        }.encode_to_vec();
        self.send(req)?;
        let _ = self.read()?;
        Ok(())
    }

    // TODO This method is for local resource managers to
    // update it's resource status, such as number of cached VMs per function
    pub fn update_resource(
        &mut self,
        manager: &LocalResourceManger
    ) -> Result<(), Box<dyn Error>> {
        let info = LocalResourceManagerInfo {
            stats: manager.get_vm_stats(),
            total_mem: manager.total_mem(),
            free_mem: manager.free_mem(),
        };
        let buf = serde_json::to_vec(&info).unwrap();
        let req = Request {
            kind: Some(request::Kind::UpdateResource(buf)),
        }.encode_to_vec();
        self.send(req)?;
        let _ = self.read()?;
        Ok(())
    }
}
