//! A fixed size pool (maybe slightly below max, max being total memory/120MB)
//! Acquire a free worker from a pool. This should always succeed because we
//! should not run out of worker threads.
//! A worker takes a reqeust and finds a VM to execute it. 

use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::net::{TcpStream};

use crate::worker::Worker;
use crate::request::Request;
use crate::message::Message;
use crate::controller::Controller;

#[derive(Debug)]
pub struct WorkerPool {
    pool: Vec<Worker>,
    req_sender: Sender<Message>,
    controller: Arc<Controller>,
}

impl WorkerPool {
    pub fn new(controller: Arc<Controller>) -> WorkerPool {
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));


        let pool_size = controller.total_mem/128;
        let mut pool = Vec::with_capacity(pool_size);

        for i in 0..pool_size {
            let cid = i as u32 + 100;
            pool.push(Worker::new(rx.clone(), controller.clone(), cid));
        }

        WorkerPool {
            pool,
            req_sender: tx,
            controller,
        }
    }
    
    pub fn get_controller(&self) -> Arc<Controller> {
        self.controller.clone()
    }

    pub fn get_sender(&self) -> Sender<Message> {
        self.req_sender.clone()
    }

    pub fn send_req(&self, req: Request, rsp_sender: Sender<Message>) {
        self.req_sender.send(Message::Request(req, rsp_sender))
            .expect("failed to send request");
    }

    pub fn send_req_tcp(&self, req: Request, rsp_sender: Arc<Mutex<TcpStream>>) {
        //TODO: better error handling
        self.req_sender.send(Message::RequestTcp(req, rsp_sender))
            .expect("failed to send request over TCP");
    }

    pub fn pool_size(&self) -> usize {
        self.pool.len()
    }
    
    pub fn shutdown(self) {
        // Shutdown all idle VMs
        self.controller.shutdown();
        // Shutdown all workers:
        // first, sending Shutdown message to each thread in the pool
        // second, wait for ack messages from all workers
        let (tx, rx) = mpsc::channel(); 
        for _ in 0..self.pool_size() {
            self.req_sender.send(Message::Shutdown(tx.clone())).expect("failed to shutdown workers");
        }
        // Worker threads may have exited while we try receiving from the channel causing
        // recv errors. We simply ignore errors.
        for _ in 0..self.pool_size() {
            match rx.recv() {
                Ok(_) => (),
                Err(_) => (),
            }
        }
        crate::unlink_unix_sockets();
    }
}
