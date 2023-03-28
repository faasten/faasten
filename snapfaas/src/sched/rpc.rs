use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::TcpStream;
use std::thread;

use crate::fs::Function;

use super::message;
use super::message::request::Kind as ReqKind;
use super::message::{Request, Response, TaskReturn};
use super::Error;

/// ping
pub fn ping(stream: &mut TcpStream) -> Result<Response, Error> {
    let req = Request {
        kind: Some(ReqKind::Ping(message::Ping {})),
    };
    message::write(stream, &req)?;
    let response = message::read_response(stream)?;
    Ok(response)
}

/// This method is for workers to retrieve a HTTP request, and
/// it is supposed to block if there's no further HTTP requests
pub fn get(stream: &mut TcpStream) -> Result<Response, Error> {
    // avoid using unstable #![feature(thread_id_value)]
    let thread_id = {
        let mut hasher = DefaultHasher::new();
        thread::current().id().hash(&mut hasher);
        hasher.finish()
    };
    let req = Request {
        kind: Some(ReqKind::GetTask(message::GetTask { thread_id })),
    };
    message::write(stream, &req)?;
    let response = message::read_response(stream)?;
    Ok(response)
}

/// This method is for workers to return the result of a HTTP request
pub fn finish(
    stream: &mut TcpStream,
    task_id: String,
    result: TaskReturn,
) -> Result<Response, Error> {
    let req = Request {
        kind: Some(ReqKind::FinishTask(message::FinishTask {
            task_id,
            result: Some(result),
        })),
    };
    message::write(stream, &req)?;
    let response = message::read_response(stream)?;
    Ok(response)
}

/// This method is for workers to invoke a function
pub fn labeled_invoke(
    stream: &mut TcpStream,
    labeled_invoke: message::LabeledInvoke,
) -> Result<(), Error> {
    let req = Request {
        kind: Some(ReqKind::LabeledInvoke(labeled_invoke)),
    };
    message::write(stream, &req)?;
    //let _ = message::read_response(stream)?;
    Ok(())
}

/// This method is for workers to invoke a function
pub fn unlabeled_invoke(
    stream: &mut TcpStream,
    unlabeled_invoke: message::UnlabeledInvoke,
) -> Result<(), Error> {
    let req = Request {
        kind: Some(ReqKind::UnlabeledInvoke(unlabeled_invoke)),
    };
    message::write(stream, &req)?;
    //let _ = message::read_response(stream)?;
    Ok(())
}

/// This method is to terminate all workers (for debug)
pub fn terminate_all(stream: &mut TcpStream) -> Result<(), Error> {
    let req = Request {
        kind: Some(ReqKind::TerminateAll(message::TerminateAll {})),
    };
    message::write(stream, &req)?;
    let _ = message::read_response(stream)?;
    Ok(())
}

/// This method is for local resource managers to update it's
/// resource status, such as number of cached VMs per function
pub fn update_resource(stream: &mut TcpStream, info: ResourceInfo) -> Result<(), Error> {
    let info = serde_json::to_vec(&info).unwrap();
    let req = Request {
        kind: Some(ReqKind::UpdateResource(message::UpdateResource { info })),
    };
    message::write(stream, &req)?;
    let _ = message::read_response(stream)?;
    Ok(())
}

/// This method is for local resrouce managers to drop itself
pub fn drop_resource(stream: &mut TcpStream) -> Result<(), Error> {
    let req = Request {
        kind: Some(ReqKind::DropResource(message::DropResource {})),
    };
    message::write(stream, &req)?;
    let _ = message::read_response(stream)?;
    Ok(())
}

#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceInfo {
    #[serde_as(as = "HashMap<serde_with::json::JsonString,_>")]
    pub stats: HashMap<Function, usize>,
    pub total_mem: usize,
    pub free_mem: usize,
}
