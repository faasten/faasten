use std::net::{SocketAddr, TcpStream};
//use std::result::Result;
use std::collections::HashMap;
//use std::sync::mpsc;
//use std::sync::mpsc::{Receiver, Sender};

use log::{debug, error};
//use serde::{Deserialize, Serialize};

use crate::fs::Function;
use crate::sched::{self, rpc::ResourceInfo};
use crate::vm::Vm;

//#[derive(Debug)]
//pub enum Message {
//    Shutdown,
//    GetVm(CacheKey, Sender<Result<Option<Vm>, Error>>),
//    ReleaseVm(Vm),
//    DeleteVm(Vm),
//    NewVm(usize, Sender<Result<usize, Error>>),
//}

#[derive(Debug)]
pub struct ResourceManager {
    cache: HashMap<Function, Vec<Vm>>,
    //receiver: Receiver<Message>,
    total_num_vms: usize, // total number of vms ever created
    total_mem: usize,
    free_mem: usize,
    sched_conn: TcpStream,
}

impl ResourceManager {
    /// create and return a ResourceManager value
    /// The ResourceManager value encapsulates the idle lists and function configs
    pub fn new(sched_addr: SocketAddr) -> Self {
        // set default total memory to free memory on the machine
        let total_mem = crate::get_machine_memory();
        let sched_conn = loop {
            debug!(
                "localrm trying to connect to the scheduler at {:?}",
                sched_addr
            );
            if let Ok(conn) = TcpStream::connect(sched_addr) {
                break conn;
            }
            std::thread::sleep(std::time::Duration::new(5, 0));
        };
        Self {
            cache: Default::default(),
            total_num_vms: 0,
            total_mem,
            free_mem: total_mem,
            sched_conn,
        }
        //let (sender, receiver) = mpsc::channel();

        //(ResourceManager {
        //    cache: Default::default(),
        //    receiver,
        //    total_num_vms: 0,
        //    total_mem,
        //    free_mem: total_mem,
        //    sched_conn: TcpStream::connect(sched_addr).expect("failed to connect to the scheduler"),
        //},
        //sender)
    }

    /// This function should only be called once before resource manager kicks off. Not supporting
    /// changing total available memory on the fly.
    pub fn set_total_mem(&mut self, mem: usize) {
        if mem > self.total_mem {
            error!(
                "Target total memory exceeds the available memory of the machine. \
                Total memory remains {}.",
                self.total_mem
            );
            return;
        }
        if mem == 0 {
            error!(
                "Total memory cannot be 0. Total memory remains {}.",
                self.total_mem
            );
            return;
        }
        self.total_mem = mem;
        self.free_mem = mem;
    }

    pub fn total_mem_in_mb(&self) -> usize {
        self.total_mem
    }

    ///// Kicks off the single thread resource manager
    //pub fn run(mut self) -> JoinHandle<()> {
    //    std::thread::spawn(move || {
    //        loop {
    //            match self.receiver.recv() {
    //                Ok(msg) => {
    //                    match msg {
    //                        Message::GetVm(key, vm_sender) => {
    //                            vm_sender.send(self.acquire_resource(&key)).expect("Failed to send VM");
    //                        },
    //                        Message::NewVm(memsize, vm_sender) => {
    //                            vm_sender.send(self.try_allocate_memory(memsize)).expect("Failed to send VM");
    //                        }
    //                        Message::ReleaseVm(vm) => {
    //                            self.release(vm);
    //                        },
    //                        Message::DeleteVm(vm) => {
    //                            self.delete(vm);
    //                        }
    //                        Message::Shutdown => {
    //                            debug!("local resource manager shutdown received");
    //                            let _ = sched::rpc::drop_resource(&mut self.sched_conn);
    //                            return;
    //                        }
    //                        _ => (),
    //                    }
    //                }
    //                Err(e) => {
    //                    panic!("ResourceManager cannot read requests: {:?}", e);
    //                }
    //            }
    //        }
    //    })
    //}

    // Try to acquire an idle VM, otherwise try to allocate a new unlaunched VM.
    // If there's not enough resources on the machine to
    // allocate a new Vm, it will try to evict an idle Vm from another
    // function's idle list, and then allocate a new unlaunched VM.
    pub fn get_cached_vm(&mut self, f: &Function) -> Option<Vm> {
        let ret = self.cache.get_mut(f).map_or(None, |l| l.pop());
        self.update_scheduler();
        ret
    }

    pub fn new_vm(&mut self, f: Function) -> Option<Vm> {
        let ret = if self.try_allocate_memory(f.memory) {
            Some(Vm::new(self.total_num_vms, f))
        } else {
            None
        };
        self.update_scheduler();
        ret
    }

    // Push the VM into the VM cache
    pub fn release(&mut self, vm: Vm) {
        if let Some(l) = self.cache.get_mut(&vm.function) {
            l.push(vm);
        } else {
            let k = vm.function.clone();
            let l = vec![vm];
            let _ = self.cache.insert(k, l);
        }
        self.update_scheduler();
    }

    pub fn delete(&mut self, vm: Vm) {
        self.free_mem += vm.function.memory;
        drop(vm); // being explicit
        self.update_scheduler();
    }

    fn update_scheduler(&mut self) {
        let stats = self
            .cache
            .iter()
            .map(|(k, v)| (k.clone(), v.len()))
            .collect();
        let info = ResourceInfo {
            stats,
            total_mem: self.total_mem,
            free_mem: self.free_mem,
        };
        let _ = sched::rpc::update_resource(&mut self.sched_conn, info);
    }

    /// proactively "reserve" requisite memory by decrementing `free_mem`.
    fn try_allocate_memory(&mut self, memory: usize) -> bool {
        if self.free_mem >= memory || self.dummy_evict(memory) {
            self.free_mem -= memory;
            self.total_num_vms += 1;
            true
        } else {
            false
        }
    }

    // Evict one or more vms to free `mem` MB of memory.
    // The function only return false when the `mem` MB is larger than the total available memory,
    // which is expected to never happen in a production system.
    fn dummy_evict(&mut self, mem: usize) -> bool {
        if self.total_mem < mem {
            return false;
        }
        let mut freed: usize = 0;
        while freed < mem {
            for l in self.cache.values_mut() {
                // TODO instead of evicting from the first non-empty list in the map,
                // collect some function popularity data and evict based on that.
                // This is where some policies can be implemented.
                if let Some(vm) = l.pop() {
                    freed += vm.function.memory;
                    self.free_mem += vm.function.memory;
                    drop(vm); // being explicit
                }
            }
        }
        true
    }
}

impl Drop for ResourceManager {
    fn drop(&mut self) {
        for l in self.cache.values_mut() {
            for vm in l.iter_mut() {
                drop(vm); // Just being explicit here, not strictly necessary
            }
        }
    }
}

//impl VmList {
//    pub fn new() -> VmList {
//        VmList {
//            num_vms: AtomicUsize::new(0),
//            list: Mutex::new(vec![]),
//        }
//    }
//
//    pub fn len(&self) -> usize {
//        self.num_vms.load(Ordering::Relaxed)
//    }
//
//    /// Pop a vm from self.list if the list is not empty.
//    /// This function blocks if it cannot grab the lock on self.list.
//    pub fn pop(&self) -> Option<Vm> {
//        match self.list.lock().expect("poisoned lock on idle list").pop() {
//            Some(v) => {
//                self.num_vms.fetch_sub(1, Ordering::Relaxed);
//                return Some(v);
//            }
//            None => return None,
//        }
//    }
//
//    /// try to grab the mutex on self.list. If try_lock() fails, just return
//    /// None instead of blocking.
//    pub fn try_pop(&self) -> Option<Vm> {
//        match self.list.try_lock() {
//            Ok(mut locked_list) => match locked_list.pop() {
//                Some(vm) => {
//                    self.num_vms.fetch_sub(1, Ordering::Relaxed);
//                    return Some(vm);
//                }
//                None => return None,
//            },
//            Err(_) => return None,
//        }
//    }
//
//    pub fn push(&self, val: Vm) {
//        self.list
//            .lock()
//            .expect("poisoned lock on idle list")
//            .push(val);
//        self.num_vms.fetch_add(1, Ordering::Relaxed);
//    }
//}
