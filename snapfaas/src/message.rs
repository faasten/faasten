use std::sync::mpsc::Sender;

use crate::request::{Request, Response};
use crate::vm::Vm;
use crate::resource_manager;
use crate::metrics::RequestTimestamps;

pub type RequestInfo = (Request, Sender<Response>, RequestTimestamps);

#[derive(Debug)]
pub enum Message {
    Shutdown,
    Request(RequestInfo),
    GetVm(String, Sender<Result<Vm, resource_manager::Error>>),
    ReleaseVm(Vm),
    DeleteVm(Vm),
}
