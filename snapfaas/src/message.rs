use std::sync::mpsc::Sender;

use crate::request::{Request, Response};
use crate::vm::Vm;
use crate::resource_manager;

#[derive(Debug)]
pub enum Message {
    Shutdown,
    Request(Request, Sender<Response>),
    GetVm(String, Sender<Result<Vm, resource_manager::Error>>),
    ReleaseVm(Vm),
    DeleteVm(Vm),
}
