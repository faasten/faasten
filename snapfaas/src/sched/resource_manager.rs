//! This resource manager maintains a global resource
//! state across worker nodes.

use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::net::{TcpStream, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

use prost::Message;
use super::{
    message,
    message::{
        Response,
        response,
    },
};


type WorkerQueue = Arc<Mutex<Vec<TcpStream>>>;

// #[derive(Debug)]
// pub struct VmList {
    // num_vms: AtomicUsize,
    // list: Mutex<Vec<Vm>>,
// }

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Node {
    pub ip: SocketAddr,
}

#[derive(Debug, Copy, Clone)]
pub struct NodeInfo(Node, usize);
// (node, number of cached vm)

#[derive(Debug)]
pub struct Worker {
    pub stream: TcpStream,
}

impl Worker {
    fn response(&mut self) -> Result<(), Box<dyn Error>> {
        let stream = &mut self.stream;
        let req = Response { kind: None }
            .encode_to_vec();
        let _ = message::send_to(stream, req)?;
        Ok(())
    }
}



// lets suppose a single rpc call is a TCP conn
#[derive(Debug, Default)]
pub struct ResourceManager {
    total_mem: usize,
    free_mem: usize,
    total_num_vms: usize, // total number of vms ever created

    pub cached: HashMap<String, Vec<NodeInfo>>,
    pub idle: HashMap<Node, Vec<Worker>>,

    // pub cached: HashMap<String, Vec<Node>>,
    // pub idle: Vec<Worker>,
}

impl ResourceManager {
    pub fn new() -> Self {
        ResourceManager {
            ..Default::default()
        }
    }

    // pub fn find_node(&mut self, function: &String) -> Option<Node> {
        // // return the first node founded
        // self.cached
            // .get_mut(function)
            // .and_then(|v| v.pop())
            // .map(|n| n.0)
    // }

    pub fn add_idle(&mut self, stream: TcpStream) {
        let node = Node {
            ip: stream.peer_addr().unwrap()
        };
        let worker = Worker { stream };
        let idle = &mut self.idle;
        if let Some(v) = idle.get_mut(&node) {
            v.push(worker);
        } else {
            idle.insert(node, vec![worker]);
        }
    }

    pub fn find_idle(&mut self, function: &String) -> Option<Worker> {
        let node = self.cached
            .get_mut(function)
            .map(|v| {
                let n = v.first_mut().unwrap();
                n.1 -= 1;
                if n.1 <= 0 {
                    v.pop().unwrap().0
                } else {
                    n.0.clone()
                }
            });
        match node {
            Some(n) => {
                let worker = self.idle
                    .get_mut(&n)
                    .and_then(|v| v.pop());
                self.idle.retain(|_, v| !v.is_empty());
                worker
            }
            None => {
                // if no such a node, simply return some woker
                let worker = self.idle
                    .values_mut()
                    .next()
                    .and_then(|v| v.pop());
                self.idle.retain(|_, v| !v.is_empty());
                worker
            }
        }
    }

    pub fn reset(&mut self) {
        for (_, workers) in self.idle.iter_mut() {
            while let Some(mut w) = workers.pop() {
                let buf = "".as_bytes().to_vec();
                let res = Response {
                    kind: Some(response::Kind::Shutdown(buf)),
                }.encode_to_vec();
                let _ = message::send_to(&mut w.stream, res);
            }
        }
        self.idle.retain(|_, v| !v.is_empty());
        // Only workers get killed,
        // local resource menagers are still alive after this
        // self.cached.retain(|_, _| false);
        // (self.total_mem, self.total_num_vms) = (0, 0);
    }


    pub fn total_num_vms(&self) -> usize {
        self.total_num_vms
    }

    pub fn total_mem(&self) -> usize {
        self.total_mem
    }

    pub fn free_mem(&self) -> usize {
        self.free_mem
    }
}
