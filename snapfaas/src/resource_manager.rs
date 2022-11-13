use std::result::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use log::{error, debug};

use crate::configs::{ResourceManagerConfig, FunctionConfig};
use crate::vm::Vm;
use crate::message::Message;
use crate::sched;

#[derive(Debug)]
pub enum Error {
    LowMemory(usize),
    NoEvictCandidate,
    InsufficientEvict,
    NoIdleVm,
    FunctionNotExist,
}

#[derive(Debug)]
pub struct VmList {
    num_vms: AtomicUsize,
    list: Mutex<Vec<Vm>>,
}

#[derive(Debug)]
pub struct ResourceManager {
    config: ResourceManagerConfig,
    idle: HashMap<String, VmList>, // from function name to a vector of VMs
    receiver: Receiver<Message>,
    pub total_num_vms: usize, // total number of vms ever created
    total_mem: usize,
    pub free_mem: usize,
    sched_addr: String,
}

impl ResourceManager {
    /// create and return a ResourceManager value
    /// The ResourceManager value encapsulates the idle lists and function configs
    pub fn new(config: ResourceManagerConfig, sched_addr: String) -> (Self, Sender<Message>) {
        let mut idle = HashMap::<String, VmList>::new();
        for (name, _) in &config.functions {
            idle.insert(name.clone(), VmList::new());
        }
        // set default total memory to free memory on the machine
        let total_mem = crate::get_machine_memory();
        let (sender, receiver) = mpsc::channel();

        (ResourceManager {
            config,
            idle,
            receiver,
            total_num_vms: 0,
            total_mem,
            free_mem: total_mem,
            sched_addr,
        },
        sender)
    }

    pub fn total_mem(&self) -> usize {
        self.total_mem
    }

    pub fn free_mem(&self) -> usize {
        self.free_mem
    }

    /// This function should only be called once before resource manager kicks off. Not supporting
    /// changing total available memory on the fly.
    pub fn set_total_mem(&mut self, mem: usize) {
        if mem > self.total_mem {
            error!("Target total memory exceeds the available memory of the machine. \
                Total memory remains {}.", self.total_mem);
            return;
        }
        if mem == 0 {
            error!("Total memory cannot be 0. Total memory remains {}.", self.total_mem);
            return;
        }
        self.total_mem = mem;
        self.free_mem = mem;
    }

    /// Kicks off the single thread resource manager
    pub fn run(mut self) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let sched = sched::Scheduler::new(self.sched_addr.clone());
            loop {
                match self.receiver.recv() {
                    Ok(msg) => {
                        match msg {
                            Message::GetVm(function, vm_sender) => {
                                vm_sender.send(self.acquire_vm(&function)).expect("Failed to send VM");
                            },
                            Message::ReleaseVm(vm) => {
                                self.release(vm);
                            },
                            Message::DeleteVm(vm) => {
                                self.delete(vm);
                            }
                            Message::Shutdown => {
                                // TODO info remote resource manager
                                debug!("local resource manager shutdown received");
                                let _ = sched.drop_resource();
                                return;
                            }
                            _ => (),
                        }
                        // TODO update info
                        use sched::resource_manager::LocalResourceManagerInfo;
                        let info = LocalResourceManagerInfo {
                            stats: self.get_vm_stats(),
                            total_mem: self.total_mem(),
                            free_mem: self.free_mem(),
                        };
                        let _ = sched.update_resource(info);
                    }
                    Err(e) => {
                        panic!("ResourceManager cannot read requests: {:?}", e);
                    }
                }
            }
        })
    }

    pub fn get_vm_stats(&self) -> HashMap<String, usize> {
        self.idle
            .iter()
            .map(|(f, v)| {
                (f.clone(), v.len())
            })
            .collect()
    }

    // Try to acquire an idle VM, otherwise try to allocate a new unlaunched VM.
    // If there's not enough resources on the machine to
    // allocate a new Vm, it will try to evict an idle Vm from another
    // function's idle list, and then allocate a new unlaunched VM.
    fn acquire_vm(
        &mut self,
        function_name: &str,
    ) -> Result<Vm, Error> {
        let func_memory = self.get_function_config(function_name)?.memory;

        self.get_idle_vm(function_name)
            .or_else(|e| {
                match e {
                   // No Idle vm for this function. Try to allocate a new vm.
                    Error::NoIdleVm => {
                        self.allocate(function_name)
                    },
                    // Not enough free memory to allocate. Try eviction
                    Error::LowMemory(_) => {
                        if self.evict(func_memory) {
                            self.allocate(function_name)
                        } else {
                            Err(Error::InsufficientEvict)
                        }
                    }
                    // Just return all other errors
                    _ => Err(e)
                }
            })
    }

    // Try to find an idle vm from the function's idle list
    fn get_idle_vm(&self, function_name: &str) -> Result<Vm, Error> {
        if let Some(idle_list) = self.idle.get(function_name) {
            return idle_list.pop().ok_or(Error::NoIdleVm);
        }
        Err(Error::FunctionNotExist)
    }

    // Push the vm onto its function's idle list
    fn release(&self, vm: Vm) {
        self.idle.get(&vm.function_name()).unwrap().push(vm); // unwrap should always work
    }

    fn delete(&mut self, vm:Vm) {
        self.free_mem += vm.memory();
        drop(vm); // being explicit
    }

    // Try to allocate a new vm for a function that is ready to boot.
    // allocate() first checks if there's enough free resources by looking at `free_mem`. If there
    // is, it proactively "reserve" requisite memory by decrementing `free_mem`.
    //
    // Allocation fail under 1 condition:
    // when there's not enough resources on the machine (Err(Error::LowMemory))
    fn allocate(
        &mut self,
        function_name: &str,
    ) -> Result<Vm, Error> {
        let function_config = self.get_function_config(function_name)?.clone();
        if self.free_mem >= function_config.memory {
            self.total_num_vms += 1;
            let id = self.total_num_vms;
            self.free_mem -= function_config.memory;

            debug!("Allocating new VM. ID: {:?}, App: {:?}", id, function_name);
            Ok(Vm::new(id, self.config.firerunner_path.clone(), function_name.to_string(), function_config, self.config.allow_network))
        } else {
            Err(Error::LowMemory(self.free_mem))
        }
    }

    // Evict one or more vms to free `mem` MB of memory.
    // The function only return false when the `mem` MB is larger than the total available memory,
    // which is expected to never happen in a production system.
    fn evict(&mut self, mem: usize) -> bool {
        if self.total_mem < mem {
            return false;
        }

        let mut freed: usize = 0;
        while freed < mem {
            for key in self.idle.keys() {
                let vmlist = self.idle.get(key).unwrap();

                // instead of evicting from the first non-empty list in the map,
                // collect some function popularity data and evict based on that.
                // This is where some policies can be implemented.
                if let Some(vm) = vmlist.try_pop() {
                    freed += vm.memory();
                    self.free_mem += vm.memory();
                    drop(vm); // being explicit
                }
            }
        }

        true
    }

    fn get_function_config(&self, function_name: &str) -> Result<&FunctionConfig, Error> {
        self.config.functions.get(function_name).ok_or(Error::FunctionNotExist)
    }
}

impl Drop for ResourceManager {
    fn drop(&mut self) {
        for key in self.idle.keys() {
            let vmlist = self.idle.get(key).unwrap();
            vmlist.list.lock().map(|mut l| {
                for vm in l.iter_mut() {
                    drop(vm); // Just being explicit here, not strictly necessary
                }
            }).expect("poisoned lock");
        }
    }
}

impl VmList {
    pub fn new() -> VmList {
        VmList {
            num_vms: AtomicUsize::new(0),
            list: Mutex::new(vec![]),
        }
    }

    /// Pop a vm from self.list if the list is not empty.
    /// This function blocks if it cannot grab the lock on self.list.
    pub fn pop(&self) -> Option<Vm> {
        match self.list.lock().expect("poisoned lock on idle list").pop() {
            Some(v) => {
                self.num_vms.fetch_sub(1, Ordering::Relaxed);
                return Some(v);
            }
            None => return None,
        }
    }

    /// try to grab the mutex on self.list. If try_lock() fails, just return
    /// None instead of blocking.
    pub fn try_pop(&self) -> Option<Vm> {
        match self.list.try_lock() {
            Ok(mut locked_list) => match locked_list.pop() {
                Some(vm) => {
                    self.num_vms.fetch_sub(1, Ordering::Relaxed);
                    return Some(vm);
                }
                None => return None,
            },
            Err(_) => return None,
        }
    }

    pub fn push(&self, val: Vm) {
        self.list
            .lock()
            .expect("poisoned lock on idle list")
            .push(val);
        self.num_vms.fetch_add(1, Ordering::Relaxed);
    }

    pub fn len(&self) -> usize {
        self.num_vms.load(Ordering::Relaxed)
    }
}
