//! This resource manager maintains a global resource
//! state across worker nodes.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, TcpStream};
use uuid::Uuid;

use crate::fs::Function;

use super::message;
use super::rpc::ResourceInfo;

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

// type WorkerId = u64;
#[derive(Debug)]
pub struct Worker {
    // pub id: WorkerId,
    pub addr: SocketAddr,
    pub conn: TcpStream,
}

/// Global resource manager
#[derive(Debug, Default)]
pub struct ResourceManager {
    // TODO Garbage collection
    pub info: HashMap<Node, NodeInfo>,
    // Locations of cached VMs for a function
    pub cached: HashMap<Function, Vec<(Node, usize)>>,
    // If no idle workers, we simply remove the entry out of
    // the hashmap, which is why we need another struct to store info
    pub idle: HashMap<Node, Vec<Worker>>,
    // For sync invoke
    pub wait_list: HashMap<Uuid, TcpStream>,
}

impl ResourceManager {
    pub fn new() -> Self {
        ResourceManager {
            ..Default::default()
        }
    }

    pub fn add_idle(&mut self, addr: SocketAddr, conn: TcpStream) {
        let node = Node(addr.ip());
        self.try_add_node(&node);
        let worker = Worker { addr, conn };
        let idle = &mut self.idle;
        if let Some(v) = idle.get_mut(&node) {
            v.push(worker);
        } else {
            idle.insert(node, vec![worker]);
        }
    }

    pub fn find_idle(&mut self, f: &Function) -> Option<Worker> {
        let info = &self.info;
        let node = self.cached.get_mut(f).and_then(|v| {
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
                let worker = self.idle.get_mut(&n).and_then(|v| v.pop());
                self.idle.retain(|_, v| !v.is_empty());
                log::debug!("find cached {:?}", worker);
                worker
            }
            None => {
                log::debug!("no cached {:?}", self.cached);
                // If no cached, simply return some worker
                let worker = self.idle.values_mut().next().and_then(|v| v.pop());
                // Mark the node dirty because it may or may not have
                // the same cached functions. This indicates an implicit
                // eviction on the remote worker node, thus we can't
                // make further decisions based on it unless confirmed
                if let Some(w) = worker.as_ref() {
                    let addr = w.addr.ip();
                    let node = Node(addr);
                    self.info.get_mut(&node).unwrap().set_dirty(true);
                }
                // Remove the entry if no more idle remains
                self.idle.retain(|_, v| !v.is_empty());
                worker
            }
        }
    }

    pub fn update(&mut self, addr: IpAddr, info: ResourceInfo) {
        log::debug!("update {:?}", info);
        let node = Node(addr);

        // Set node to not dirty bc we are sure of its state
        let success = self.try_add_node(&node);
        if !success {
            self.info.get_mut(&node).unwrap().set_dirty(false);
        }

        // Update mem info as well
        let nodeinfo = self.info.get_mut(&node).unwrap();
        nodeinfo.total_mem = info.total_mem;
        nodeinfo.free_mem = info.free_mem;

        // Update number of cached VMs per funciton
        for (k, num_cached) in info.stats {
            let nodes = self.cached.get_mut(&k);
            match nodes {
                Some(nodes) => {
                    let n = nodes.iter_mut().find(|&&mut n| n.0 == node);
                    if let Some(n) = n {
                        n.1 = num_cached;
                    } else {
                        nodes.push((node.clone(), num_cached));
                    }
                    nodes.retain(|n| n.1 > 0);
                }
                None => {
                    if num_cached > 0 {
                        let k = k.clone();
                        let v = vec![(node.clone(), num_cached)];
                        let _ = self.cached.insert(k, v);
                    }
                }
            }
        }
    }

    pub fn remove(&mut self, addr: IpAddr) {
        use message::response::Kind as ResKind;
        let node = Node(addr);
        // They must have no busy worker
        for (_, v) in self.cached.iter_mut() {
            if let Some(pos) = v.iter().position(|&n| n.0 == node) {
                // This doesn't preserve ordering
                v.swap_remove(pos);
                v.retain(|n| n.1 != 0);
            }
        }
        self.info.remove(&node);
        if let Some(mut workers) = self.idle.remove(&node) {
            while let Some(mut w) = workers.pop() {
                let _ = message::write(
                    &mut w.conn,
                    &message::Response {
                        kind: Some(ResKind::Terminate(message::Terminate {})),
                    },
                );
            }
        }
    }

    pub fn remove_all(&mut self) {
        let nodes = self.info.keys().cloned().collect::<Vec<_>>();
        for node in nodes.into_iter() {
            self.remove(node.0)
        }
    }

    fn try_add_node(&mut self, node: &Node) -> bool {
        let has_node = self.info.contains_key(&node);
        if !has_node {
            self.info.insert(node.clone(), NodeInfo::new(node.clone()));
        }
        !has_node
    }
}
