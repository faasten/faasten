//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::thread;
use std::thread::JoinHandle;
use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver, SendError};
use crate::request::Request;
use std::time::Duration;

/// From JoinHandle we can get the &Thread which then gives us ThreadId and
/// park() function. We can't peel off the JoinHandle to get Thread because
/// JoinHandle struct owns Thread as a field.
pub struct Worker {
    thread: JoinHandle<()>,
//    curr_req: Option<Request>,
    req_sender: Sender<Request>
}

impl Worker {
    pub fn new() -> Worker {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let req = rx.recv();
            println!("req (worker): {:?}", req);
        });
        Worker {
            thread: handle,
            req_sender: tx,
        }
    }

    pub fn send_req(&self, req: Request) -> Result<(), SendError<Request>> {
        return self.req_sender.send(req);
    }
}
