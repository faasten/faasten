//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::thread;
use std::thread::JoinHandle;
use std::sync::Mutex;
use std::sync::Arc;
use std::sync::mpsc::{Sender, Receiver, SendError};
use std::time::Duration;
use std::io::Result;

use log::{error, info};

use crate::request::Request;
use crate::message::Message;

/// From JoinHandle we can get the &Thread which then gives us ThreadId and
/// park() function. We can't peel off the JoinHandle to get Thread because
/// JoinHandle struct owns Thread as a field.
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
    pub fn new(receiver: Arc<Mutex<Receiver<Message>>>) -> Worker {

        let handle = thread::spawn(move || {

                let id = thread::current().id();
                let mut state = State::WaitForMsg;
                loop {
                    match state {
                        State::WaitForMsg=> {
                            let msg: Message = receiver.lock().unwrap().recv().unwrap();
                            state = Worker::process_msg(msg);
                            continue;
                        },
                        State::Shutdown => {
                            info!("Worker {:?}: shutdown received", id);
                            return;
                        },
                        State::Response => {
                            state = State::WaitForMsg;
                        },
                        State::ReqFail => {
                            state = State::WaitForMsg;
                        }
                    }
                }
            
        });

        Worker {
            thread: handle,
        }
    }

    pub fn process_msg(msg: Message) -> State {
        match msg {
            Message::Shutdown => {
                return State::Shutdown;
            },
            Message::Request(req, rsp_sender) => {
                match Worker::process_req(req) {
                    Ok(rsp) => {
                        //info!("process result: {:?}", rsp);
                        rsp_sender.send(Message::Response(rsp));
                        return State::Response;
                    },
                    Err(err) => {
                        info!("Request failed: {:?}", err);
                        return State::ReqFail;
                    },
                }
            }
            _ => {
                error!("Invalid worker message");
                return State::WaitForMsg;
            }
        }
    }

    pub fn process_req(req: Request) -> Result<String> {
        let id = thread::current().id();
        info!("worker {:?} recv: {:?}", id, req);
        return Ok(String::from("success"));
    }

    /*
    pub fn transition(&mut self, s: State) {
        self.state = s;
    }

    fn wait_for_req(rx: Receiver<Request>) {
        let req = rx.recv();

    }

    fn echo_req(req: &Request) {
        println!("req (worker): {:?}", req);
    }

    pub fn send_req(self, req: Request) -> Result<(), SendError<(Request, Worker)>> {
        if self.state != State::WaitForReq {
            panic!("worker not in WaitForReq state");
        }
        return self.req_sender.send((req,self));
    }
    */
}
