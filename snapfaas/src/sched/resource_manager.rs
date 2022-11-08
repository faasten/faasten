//! This resource manager maintains a global resource
//! state across worker nodes.

use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::net::{TcpStream, SocketAddr, IpAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

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
pub struct Node(IpAddr);

#[derive(Debug, Copy, Clone)]
pub struct NodeInfo {
    pub node: Node,
    pub num_cached: usize,
}
// pub struct NodeInfo(Node, usize);
// (node, number of cached vm)

type WorkerId = u64;

#[derive(Debug)]
pub struct Worker {
    // pub id: WorkerId,
    pub stream: TcpStream, // connection on demand
}

// impl Worker {
    // fn response(&mut self) -> Result<(), Box<dyn Error>> {
        // let stream = &mut self.stream;
        // let req = Response { kind: None }
            // .encode_to_vec();
        // let _ = message::send_to(stream, req)?;
        // Ok(())
    // }
// }


// lets suppose a single rpc call is a TCP conn
#[derive(Debug, Default)]
pub struct ResourceManager {
    // TODO per node
    // total_mem: usize,
    // free_mem: usize,
    // total_num_vms: usize, // total number of vms ever created

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
        let addr = stream.peer_addr().unwrap();
        let node = Node(addr.ip());
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
                            let fst = v.first_mut().unwrap();
                            fst.num_cached -= 1;
                            if fst.num_cached <= 0 {
                                let fst = v.pop().unwrap();
                                fst.node
                            } else {
                                fst.node.clone()
                            }
                        });
        match node {
            Some(n) => {
                let worker = self.idle
                                .get_mut(&n)
                                .and_then(|v| v.pop());
                self.idle.retain(|_, v| !v.is_empty());
                log::debug!("find cached {:?}", worker);
                worker
            }
            None => {
                log::debug!("no cached {:?}", self.cached);
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
                };
                let _ = message::write(&mut w.stream, res);
            }
        }
        self.idle.retain(|_, v| !v.is_empty());
        // TODO Only workers get killed, meaning that
        // local resource menagers are still alive after this
        // self.cached.retain(|_, _| false);
        // (self.total_mem, self.total_num_vms) = (0, 0);
    }

    pub fn update(&mut self, addr: IpAddr, info: LocalResourceManagerInfo) {
        log::debug!("update {:?}", info);

        let node = Node(addr);
        for (f, n) in info.stats.into_iter() {
            let nodes = self.cached.get_mut(&f);
            match nodes {
                Some(nodes) => {
                    let nodeinfo = nodes
                                    .iter_mut()
                                    .find(|&&mut n| n.node == node);
                    if let Some(nodeinfo) = nodeinfo {
                        nodeinfo.num_cached = n;
                    } else {
                        let nodeinfo = NodeInfo {
                            node: node.clone(),
                            num_cached: n,
                        };
                        nodes.push(nodeinfo);
                    }
                }
                None => {
                    let nodeinfo = NodeInfo {
                        node: node.clone(),
                        num_cached: n,
                    };
                    let function = f.clone();
                    let _ = self.cached
                                .insert(function, vec![nodeinfo]);
                }
            }
        }
    }

    // pub fn total_num_vms(&self) -> usize {
        // self.total_num_vms
    // }

    // pub fn total_mem(&self) -> usize {
        // self.total_mem
    // }

    // pub fn free_mem(&self) -> usize {
        // self.free_mem
    // }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalResourceManagerInfo {
    pub stats: HashMap<String, usize>,
    pub total_mem: usize,
    pub free_mem: usize,
}



