//! A fixed size pool (maybe slightly below max, max being total memory/120MB)
//! Acquire a free worker from a pool. This should always succeed because we
//! should not run out of worker threads.
//! A worker takes a reqeust and finds a VM to execute it. 

use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{Sender, Receiver, SendError};

use log::{error, warn, info};

use crate::worker::Worker;
use crate::request::Request;
use crate::message::Message;
use crate::controller::Controller;

const DEFAULT_NUM_WORKERS: usize = 10;

pub struct WorkerPool {
    pool: Vec<Worker>,
    max_num_workers: usize,
    req_sender: Sender<Message>,
    controller: Arc<Controller>,
}

impl WorkerPool {
    pub fn new(controller: Controller) -> WorkerPool {
        let mut pool = Vec::with_capacity(DEFAULT_NUM_WORKERS);

        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));

        let controller = Arc::new(controller);


        for _ in 0..DEFAULT_NUM_WORKERS {
            pool.push(Worker::new(rx.clone(), controller.clone()));
        }

        WorkerPool {
            pool: pool,
            max_num_workers: DEFAULT_NUM_WORKERS,
            req_sender: tx,
            controller: controller,
        }
    }

    pub fn send_req(&self, req: Request, rsp_sender: Sender<Message>) {
        self.req_sender.send(Message::Request(req, rsp_sender));
    }

    pub fn shutdown(self) {

        for _ in &self.pool {
            self.req_sender.send(Message::Shutdown);
        }

        for w in self.pool {
            let id = w.thread.thread().id();
            if let Err(e) = w.thread.join() {
                error!("worker thread {:?} panicked {:?}", id, e);
            }
        }
    }

    
}
