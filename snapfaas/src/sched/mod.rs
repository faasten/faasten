pub mod rpc_server;
pub mod resource_manager;
pub mod message;
pub mod rpc;

use std::sync::mpsc::Sender;
use uuid::Uuid;
use message::{LabeledInvoke, UnlabeledInvoke};

pub type RequestInfo = (message::LabeledInvoke, Sender<String>);

#[derive(Debug)]
pub enum Error {
    Rpc(prost::DecodeError),
    TaskSend(std::sync::mpsc::SendError<Task>),
    StreamConnect(std::io::Error),
    StreamRead(std::io::Error),
    StreamWrite(std::io::Error),
    Other(String),
}

#[derive(Debug)]
pub enum Task {
    Invoke(Uuid, LabeledInvoke),
    InvokeInsecure(Uuid, UnlabeledInvoke),
    Terminate,
}
