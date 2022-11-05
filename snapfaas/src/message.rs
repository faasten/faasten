use std::sync::mpsc::Sender;

use crate::request::{Request, Response};
use crate::vm::Vm;
use crate::resource_manager;
use crate::metrics::RequestTimestamps;

pub type RequestInfo2 = (Request, Sender<Response>, RequestTimestamps);
pub type RequestInfo = (Request, RequestTimestamps);

#[derive(Debug)]
pub enum Message {
    Shutdown,
    Request(RequestInfo),
    Request2(RequestInfo2),
    GetVm(String, Sender<Result<Vm, resource_manager::Error>>),
    ReleaseVm(Vm),
    DeleteVm(Vm),
}
