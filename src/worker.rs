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
#[derive(Debug)]
pub struct Worker {
    thread: JoinHandle<()>,
//    curr_req: Option<Request>,
    pub req_sender: Sender<(Request, Worker)>,
    state: State,
}

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    WaitForReq,
    Done
}

impl Worker {
    pub fn new() -> Worker {
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            let req = rx.recv();
            println!("req (worker): {:?}", req);
            
            return;
        });

        Worker {
            thread: handle,
            req_sender: tx,
            state: State::WaitForReq
        }
    }

    pub fn transition(&mut self, s: State) {
        self.state = s;
    }

    fn wait_for_req(rx: Receiver<Request>) {
        let req = rx.recv();

    }

    fn echo_req(req: &Request) {
        println!("req (worker): {:?}", req);
    }

    /*
    pub fn send_req(self, req: Request) -> Result<(), SendError<(Request, Worker)>> {
        if self.state != State::WaitForReq {
            panic!("worker not in WaitForReq state");
        }
        return self.req_sender.send((req,self));
    }
    */
}
