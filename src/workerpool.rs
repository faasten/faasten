//! A fixed size pool (maybe slightly below max, max being total memory/120MB)
//! Acquire a free worker from a pool. This should always succeed because we
//! should not run out of worker threads.
//! A worker takes a reqeust and finds a VM to execute it. 

use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{Sender, Receiver, SendError};
use std::net::{TcpStream};
use std::collections::HashMap;
use std::thread::JoinHandle;

use log::{error, warn, info};

use crate::vsock::*;
use crate::worker::Worker;
use crate::request::Request;
use crate::message::Message;
use crate::controller::Controller;

#[derive(Debug)]
pub struct WorkerPool {
    pool: Vec<Worker>,
    req_sender: Sender<Message>,
    controller: Arc<Controller>,
    vsock_closer: VsockCloser,
    vsock_thread_handle: JoinHandle<()>,
}

impl WorkerPool {
    pub fn new(controller: Arc<Controller>) -> WorkerPool {
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));


        let pool_size = controller.total_mem/128;
        let mut pool = Vec::with_capacity(pool_size);
        let mut vsock_stream_senders = HashMap::with_capacity(pool_size);

        for i in 0..pool_size {
            let (sender, receiver) = mpsc::channel();
            let cid = i as u32 + 100;
            pool.push(Worker::new(rx.clone(), controller.clone(), receiver, cid));
            vsock_stream_senders.insert(cid, sender);
        }

        let mut vsock_listener = VsockListener::bind(VMADDR_CID_ANY, VSOCKPORT)
                .expect("VsockListener::bind");
        let vsock_closer = vsock_listener.closer();
        let vsock_thread_handle = std::thread::spawn(move || {
            loop {
                match vsock_listener.accept() {
                    Ok((vsock_stream, vsock_addr)) =>
                        vsock_stream_senders.get(&vsock_addr.cid).expect("unknown cid")
                            .send(vsock_stream).expect("failed to send vsock connection"),
                    Err(_) =>
                        break,
                }
            }
        });

        WorkerPool {
            pool: pool,
            req_sender: tx,
            controller: controller,
            vsock_closer,
            vsock_thread_handle,
        }
    }

    pub fn send_req(&self, req: Request, rsp_sender: Sender<Message>) {
        self.req_sender.send(Message::Request(req, rsp_sender))
            .expect("failed to send request");
    }

    pub fn send_req_tcp(&self, req: Request, rsp_sender: Arc<Mutex<TcpStream>>) {
        //TODO: better error handling
        self.req_sender.send(Message::Request_Tcp(req, rsp_sender))
            .expect("failed to send request over TCP");
    }

    pub fn pool_size(&self) -> usize {
        self.pool.len()
    }
    
    /// shutdown the workerpool
    /// This involves
    /// 1. sending Shutdown message to each thread in the pool
    /// 2. wait for all threads in the pool to terminate
    pub fn shutdown(self) {
        self.vsock_closer.close().expect("failed to close vsock listener");
        for _ in &self.pool {
            self.req_sender.send(Message::Shutdown).expect("failed to shutdown workers");
        }

        if let Err(e) = self.vsock_thread_handle.join() {
            error!("failed to join vsock listener thread");
        }
        for w in self.pool {
            let id = w.thread.thread().id();
            if let Err(e) = w.thread.join() {
                error!("worker thread {:?} panicked {:?}", id, e);
            }
        }
    }
}
