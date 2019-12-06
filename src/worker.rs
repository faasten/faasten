//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::result::Result;
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use log::{error, info};

use crate::controller::Controller;
use crate::message::Message;
use crate::request::Request;
use crate::vm::Vm;

#[derive(Debug)]
pub struct Worker {
    pub thread: JoinHandle<()>,
}

#[derive(Debug)]
pub enum State {
    WaitForMsg,
    Shutdown,
    Response,
    ReqFail,
}

impl Worker {
    pub fn new(receiver: Arc<Mutex<Receiver<Message>>>, ctr: Arc<Controller>) -> Worker {
        let handle = thread::spawn(move || {
            let id = thread::current().id();

            loop {
                let msg: Message = receiver.lock().unwrap().recv().unwrap();
                match msg {
                    Message::Shutdown => {
                        info!("Thread {:?} shutdown received", id);
                        return;
                    }
                    Message::Request(req, rsp_sender) => match Worker::process_req(req, &ctr) {
                        Ok(rsp) => {
                            if let Err(e) = rsp_sender.send(Message::Response(rsp)) {
                                error!("[thread: {:?}] response failed to send: {:?}", id, e);
                            }
                        }
                        Err(err) => info!("Request failed: {:?}", err),
                    },
                    _ => {
                        error!("Invalid message to thread {:?}: {:?}", id, msg);
                    }
                }
            }
        });

        Worker { thread: handle }
    }

    /// Send a request to a Vm, wait for Vm's response and return the result.
    /// A worker will first try to acquire an idle Vm to handle the request.
    /// If there are no idle Vms for the particular request, it will try to
    /// allocate a new Vm. If there's not enough resources on the machine to
    /// allocate a new Vm, it will try to evict an idle Vm from another
    /// function's idle list, and then allocate a new Vm.
    /// After processing the request, the worker will push its Vm into the idle
    /// list of its function.
    pub fn process_req(req: Request, ctr: &Arc<Controller>) -> Result<String, String> {
        let id = thread::current().id();

        let function_name = req.function.clone();
        let func_config = ctr.get_function_config(&function_name);
        if func_config.is_none() {
            error!("[Worker {:?}] Unknown request: {:?}", id, req);
            return Err(String::from("Unknown request"));
        }
        let func_config = func_config.unwrap(); // unwrap should be safe here

        info!("[Worker {:?}] recv: {:?}", id, req);
        info!("[Worker {:?}] function config: {:?}", id, func_config);

        let vm = ctr
            .get_idle_vm(&function_name)
            .or_else(|| {ctr.allocate(func_config)})
            .or_else(|| {ctr.evict_and_allocate(func_config.memory, func_config)});

        let res = match vm {
            None => Err(String::from("Resource exhaustion")),
            Some(vm) => {
                let res = vm.process_req(req);
                ctr.release(&function_name, vm);
                res
            }
        };

        return res;
    }
}
