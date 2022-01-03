use std::result::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use log::{error, debug};

use crate::*;
use crate::configs::{ResourceManagerConfig, FunctionConfig};
use crate::vm::Vm;
use crate::message::Message;

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
}

impl ResourceManager {
    /// create and return a ResourceManager value
    /// The ResourceManager value encapsulates the idle lists and function configs
    pub fn new(config: ResourceManagerConfig) -> (Self, Sender<Message>) {
        let mut idle = HashMap::<String, VmList>::new();
        for (name, _) in &config.functions {
            idle.insert(name.clone(), VmList::new());
        }
        // set default total memory to free memory on the machine
        let total_mem = get_machine_memory();
        let (sender, receiver) = mpsc::channel();
        
        (ResourceManager {
            config,
            idle,
            receiver,
            total_num_vms: 0,
            total_mem,
            free_mem: total_mem,
        },
        sender)
    }

    pub fn total_mem(&self) -> usize {
        self.total_mem
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
                                return;
                            }
                            _ => (),
                        }
                    }
                    Err(e) => {
                        panic!("ResourceManager cannot read requests: {:?}", e);
                    }
                }
            }
        })
    }

    // Try to acquire an idle VM, otherwise try to allocate a new unlaunched VM.
    // If there's not enough resources on the machine to
    // allocate a new Vm, it will try to evict an idle Vm from another
    // function's idle list, and then allocate a new unlaunched VM.
    fn acquire_vm(
        &mut self,
        function_name: &str,
    )-> Result<Vm, Error> {
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
}

// check concurrent allocate() correctly decrements ResourceManager.free_mem
// check vms are correctly pushed concurrently to idle lists after use with release()
// check get_idle_vm() returns unique vms to concurrent calls
#[cfg(test)]
mod tests {
    use super::*;

    use std::os::unix::io::{FromRawFd, RawFd};
    use std::thread;
    use std::sync::Arc;

    const DUMMY_APP_NAME: &str = "hello";

    fn mock_unixlistener() -> UnixListener {
        unsafe{ UnixListener::from_raw_fd(23425 as RawFd) }
    }

    /// Helper function to create a ResourceManager value with `mem` amount of memory
    /// If `mem` is set to 0, the amount of memory is set to the total memory
    /// of the machine.
    fn build_controller(mem: usize) -> ResourceManager {
        let ctr_config = ResourceManagerConfig::new("./resources/example-controller-config.yaml");

        let mut ctr = ResourceManager::new(ctr_config);
        if mem != 0 {
            ctr.total_mem = mem;
            ctr.free_mem = AtomicUsize::new(mem);
        }
        return ctr;
    }

    #[test]
    /// Allocate should correctly increment `total_num_vms` and decrement `free_mem`.
    fn test_allocate() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;
        let num_vms = 10;

        for _ in 0..num_vms {
            controller.allocate(
                DUMMY_APP_NAME,
                &mock_unixlistener(),
                0,
                "",
            ).unwrap();
        }

        assert_eq!(controller.total_num_vms.load(Ordering::Relaxed), num_vms);
        assert_eq!(
            controller.free_mem.load(Ordering::Relaxed),
            total_mem - num_vms * 128
        );
    }

    #[test]
    /// Allocate should correctly increment `total_num_vms` and decrement `free_mem`.
    fn test_allocate_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;
        let num_vms = 10;

        let sctr = Arc::new(controller);

        let mut handles = vec![];

        for _ in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                ctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ).unwrap();
            });
            handles.push(h);
        }

        for h in handles {
            h.join().expect("Couldn't join on the thread");
        }

        assert_eq!(sctr.total_num_vms.load(Ordering::Relaxed), num_vms);
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            total_mem - num_vms * 128
        );
    }

    #[test]
    /// Allocate should fail when resources are exhausted.
    /// `total_num_vms` and `free_mem` should also be correct.
    fn test_allocate_resource_limit() {
        let controller = build_controller(1024);

        for _ in 0..8 {
            if let Err(_) = controller.allocate(
                DUMMY_APP_NAME,
                &mock_unixlistener(),
                0,
                "",
            ) {
                panic!("allocate failed before exhausting resources");
            }
        }

        if let Ok(_) = controller.allocate(
                DUMMY_APP_NAME,
                &mock_unixlistener(),
                0,
                "",
            ) {
            panic!("allocate succeeds after exhausting resources");
        }

        assert_eq!(controller.total_num_vms.load(Ordering::Relaxed), 8);
        assert_eq!(controller.free_mem.load(Ordering::Relaxed), 0);
    }

    #[test]
    /// Allocate should fail when resources are exhausted.
    /// `total_num_vms` and `free_mem` should also be correct.
    fn test_allocate_resource_limit_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let num_vms = total_mem / sctr.get_function_config(DUMMY_APP_NAME).unwrap().memory;

        let mut handles = vec![];

        for _ in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                if let Err(_) = ctr.allocate(
                        DUMMY_APP_NAME,
                        &mock_unixlistener(),
                        0,
                        "",
                    ) {
                    panic!("allocate failed before exhausting resources");
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join().expect("Couldn't join on the thread");
        }

        for _ in 0..num_vms {
            if let Ok(_) = sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                panic!("allocate succeeds after exhausting resources");
            }
        }

        assert_eq!(sctr.total_num_vms.load(Ordering::Relaxed), num_vms);
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            total_mem - num_vms * 128
        );
    }

    #[test]
    /// release should add vm to its idle list and increment the list's `num_vms`
    fn test_release() {
        let controller = build_controller(1024);

        assert_eq!(
            controller
                .idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            controller
                .idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .list
                .lock()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(controller.free_mem.load(Ordering::Relaxed), 1024);

        for _ in 0..8 {
            match controller.allocate(
                DUMMY_APP_NAME,
                &mock_unixlistener(),
                0,
                "",
            ) {
                Ok(vm) => controller.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            controller
                .idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            8
        );
        assert_eq!(
            controller
                .idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .list
                .lock()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(controller.free_mem.load(Ordering::Relaxed), 0);
    }

    #[test]
    /// release should add vm to its idle list and increment the list's `num_vms`
    fn test_release_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();
        let num_vms = total_mem / c.memory;

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );
        assert_eq!(sctr.free_mem.load(Ordering::Relaxed), total_mem);

        let mut handles = vec![];

        for _ in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                match ctr.allocate(
                        DUMMY_APP_NAME,
                        &mock_unixlistener(),
                        0,
                        "",
                    ) {
                    Ok(vm) => ctr.release(DUMMY_APP_NAME, vm),
                    Err(_) => panic!("allocate failed before exhausting resources"),
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join().expect("Couldn't join on the thread");
        }

        for _ in 0..num_vms {
            if let Ok(_) = sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                panic!("allocate succeeds after exhausting resources");
            }
        }
        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            total_mem - 128 * num_vms
        );
    }

    #[test]
    /// get_idle_vm should remove a vm from its idle list, return Some(vm) and
    /// decrement the list's `num_vms`. It should return None if the idle list is empty.
    fn test_get_idle() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                Ok(vm) => sctr.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );

        for _ in 0..num_vms {
            if let Err(_) = sctr.get_idle_vm(DUMMY_APP_NAME) {
                panic!("idle list should not be empty")
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Ok(_) = sctr.get_idle_vm(DUMMY_APP_NAME) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );
    }

    #[test]
    /// get_idle_vm should remove a vm from its idle list, return Some(vm) and
    /// decrement the list's `num_vms`. It should return None if the idle list is empty.
    fn test_get_idle_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                Ok(vm) => sctr.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );

        let mut handles = vec![];

        for _ in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                if let Err(_) = ctr.get_idle_vm(DUMMY_APP_NAME) {
                    panic!("idle list should not be empty")
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join().expect("Couldn't join on the thread");
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Ok(_) = sctr.get_idle_vm(DUMMY_APP_NAME) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );
    }

    #[test]
    /// evict should remove a vm from its idle list and increment `free_mem`.
    fn test_eviction_single_vm() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                Ok(vm) => sctr.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            sctr.total_mem - 128 * num_vms
        );

        for _ in 0..num_vms {
            if !sctr.evict(c.memory) {
                panic!("idle list should not be empty")
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Ok(_) = sctr.get_idle_vm(DUMMY_APP_NAME) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );
    }

    #[test]
    /// evict should remove a vm from its idle list and increment `free_mem`.
    fn test_eviction_single_vm_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                Ok(vm) => sctr.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            sctr.total_mem - c.memory * num_vms
        );

        let mut handles = vec![];

        for _ in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                let c = ctr.get_function_config(DUMMY_APP_NAME).unwrap();
                if !ctr.evict(c.memory) {
                    panic!("idle list should not be empty")
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join().expect("Couldn't join on the thread");
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Ok(_) = sctr.get_idle_vm(DUMMY_APP_NAME) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );
    }

    #[test]
    /// evict should continue removing vms from its idle list and incrementing
    /// `free_mem` until at least `mem` amount of memory are freed.
    fn test_eviction_multi_vms() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                Ok(vm) => sctr.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            sctr.total_mem - c.memory * num_vms
        );

        let num_evict = num_vms / 2;

        for _ in 0..num_evict {
            if !sctr.evict(c.memory * 2) {
                panic!("idle list should not be empty")
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms - num_evict * 2
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms - num_evict * 2
        );
    }

    #[test]
    fn test_eviction_multi_vms_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                Ok(vm) => sctr.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            sctr.total_mem - c.memory * num_vms
        );

        let num_evict = num_vms / 2;

        let mut handles = vec![];
        for _ in 0..num_evict {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                let c = ctr.get_function_config(DUMMY_APP_NAME).unwrap();
                if !ctr.evict(c.memory * 2) {
                    panic!("idle list should not be empty")
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join().expect("Couldn't join on the thread");
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms - num_evict * 2
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms - num_evict * 2
        );
    }

    #[test]
    /// evict should fail and return if there's nothing idle to evict
    fn test_eviction_failure_not_block() {
        let controller = build_controller(0);
        let num_vms = 1;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config(DUMMY_APP_NAME).unwrap();

        for _ in 0..num_vms {
            match sctr.allocate(
                    DUMMY_APP_NAME,
                    &mock_unixlistener(),
                    0,
                    "",
                ) {
                Ok(vm) => sctr.release(DUMMY_APP_NAME, vm),
                Err(_) => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            sctr.total_mem - c.memory * num_vms
        );

        if sctr.evict(c.memory * 2) {
            panic!("eviction should fail")
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );

        if sctr.evict(c.memory) {
            panic!("eviction should fail")
        }

        assert_eq!(
            sctr.idle
                .get(DUMMY_APP_NAME)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(DUMMY_APP_NAME).unwrap().list.lock().unwrap().len(),
            0
        );
    }
}
