//! This resource manager maintains a global resource
//! state across worker nodes.

// use std::error::Error;
// use std::sync::{Arc, Mutex};
// use std::thread;
use std::net::{TcpStream, IpAddr};
// use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use super::{
    message,
    message::{
        Response,
        response,
    },
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Node(IpAddr);

#[derive(Debug)]
pub struct NodeInfo {
    pub node: Node,
    total_mem: usize,
    free_mem: usize,
    dirty: bool,
}

impl NodeInfo {
    fn new(node: Node) -> Self {
        NodeInfo {
            node,
            dirty: false,
            total_mem: Default::default(),
            free_mem: Default::default(),
        }
    }

    fn dirty(&self) -> bool {
        self.dirty
    }

    fn set_dirty(&mut self, v: bool) {
        self.dirty = v;
    }
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


/// Global resource manager
#[derive(Debug, Default)]
pub struct ResourceManager {
    // TODO garbage collection
    pub info: HashMap<Node, NodeInfo>,
    // Locations of cached VMs for a function
    pub cached: HashMap<String, Vec<(Node, usize)>>,
    // If no idle workers, we simply remove the entry out of
    // the hashmap, which is why we need another struct to store info
    pub idle: HashMap<Node, Vec<Worker>>,
}

impl ResourceManager {
    pub fn new() -> Self {
        ResourceManager {
            ..Default::default()
        }
    }

    pub fn add_idle(&mut self, stream: TcpStream) {
        let addr = stream.peer_addr().unwrap();
        let node = Node(addr.ip());
        let _ = self.try_add_node(&node);
        let worker = Worker { stream };
        let idle = &mut self.idle;
        if let Some(v) = idle.get_mut(&node) {
            v.push(worker);
        } else {
            idle.insert(node, vec![worker]);
        }
    }

    pub fn find_idle(&mut self, function: &String) -> Option<Worker> {
        let info = &self.info;
        let node = self.cached
                    .get_mut(function)
                    .and_then(|v| {
                        let fst = v
                            .iter_mut()
                            // Find the first safe node
                            .find(|n| {
                                let i = info.get(&n.0).unwrap();
                                !i.dirty()
                            })
                            // Update cached number for this node
                            // because we are going to use one of
                            // it's idle workers. A cached VM always
                            // implies an idle worker, but not the opposite
                            .map(|n| {
                                n.1 -= 1;
                                n.0.clone()
                            });
                        // Remove the entry if no more cached VM remains
                        v.retain(|n| n.1 != 0);
                        fst
                    });
        // Find idle worker
        // FIXME assume that all workers can handle any function
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
                // If no cached, simply return some worker
                let worker = self.idle
                                .values_mut()
                                .next()
                                .and_then(|v| v.pop());
                // Mark the node dirty because it may or may not have
                // some cached function. This indicates an implicit
                // eviction on the remote worker node, thus we can't
                // further make decisions based on it unless confirmed
                if let Some(w) = worker.as_ref() {
                    let addr = w.stream
                                .peer_addr().unwrap().ip();
                    let node = Node(addr);
                    self.info
                        .get_mut(&node)
                        .unwrap()
                        .set_dirty(true);
                }
                // Remove the entry if no more idle remains
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

        let success = self.try_add_node(&node);
        if !success {
            self.info
                .get_mut(&node)
                .unwrap()
                .set_dirty(false);
        }

        // TODO update mem info as well
        let nodeinfo = self.info
                            .get_mut(&node)
                            .unwrap();
        nodeinfo.total_mem = info.total_mem;
        nodeinfo.free_mem = info.free_mem;

        for (f, num_cached) in info.stats.into_iter() {
            let nodes = self.cached.get_mut(&f);
            match nodes {
                Some(nodes) => {
                    let n = nodes
                            .iter_mut()
                            .find(|&&mut n| n.0 == node);
                    if let Some(n) = n {
                        n.1 = num_cached;
                    } else {
                        nodes.push((node.clone(), num_cached));
                    }
                }
                None => {
                    let f = f.clone();
                    let v = vec![(node.clone(), num_cached)];
                    let _ = self.cached.insert(f, v);
                }
            }
        }
    }

    pub fn remove(&mut self, addr: IpAddr) {
        let node = Node(addr);
        // They must have no idle worker
        for (_, v) in self.cached.iter_mut() {
            if let Some(pos) = v.iter().position(|&n| n.0 == node) {
                // This doesn't preserve ordering
                v.swap_remove(pos);
            }
        }
        self.info.remove(&node);
        self.idle.remove(&node);
    }

    fn try_add_node(&mut self, node: &Node) -> bool {
        let has_node = self.info.contains_key(&node);
        if !has_node {
            self.info.insert(
                node.clone(),
                NodeInfo::new(node.clone())
            );
        }
        !has_node
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalResourceManagerInfo {
    pub stats: HashMap<String, usize>,
    pub total_mem: usize,
    pub free_mem: usize,
}



