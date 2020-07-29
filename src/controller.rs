use std::result::Result;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::sync::Mutex;
use std::os::unix::net::UnixListener;

use log::{error, trace};
use serde_yaml;

use crate::*;
use crate::configs::{ControllerConfig, FunctionConfig};
use crate::vm::Vm;


const EVICTION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct VmList {
    num_vms: AtomicUsize,
    list: Mutex<Vec<Vm>>,
}

#[derive(Debug)]
pub struct Controller {
    controller_config: ControllerConfig,
    function_configs: BTreeMap<String, FunctionConfig>,
    idle: HashMap<String, VmList>, // from function name to a vector of VMs
    pub total_num_vms: AtomicUsize, // total number of vms ever created
    pub total_mem: usize,
    pub free_mem: AtomicUsize,
}

#[derive(Debug)]
pub enum Error {
    LowMemory(usize),
    StartVm(vm::Error),
    VmReqProcess(vm::Error),
    NoEvictCandidate,
    InsufficientEvict,
    NoIdleVm,
    FunctionNotExist,
}

impl Controller {
    /// create and return a Controller value
    /// The Controller value encapsulates the idle lists and function configs
    pub fn new(ctr_config: ControllerConfig) -> Option<Controller> {
        let mut function_configs = BTreeMap::<String, FunctionConfig>::new();
        let mut idle = HashMap::<String, VmList>::new();
        match open_url(&ctr_config.function_config) {
            Ok(fd) => {
                let apps: serde_yaml::Result<Vec<FunctionConfig>> = serde_yaml::from_reader(fd);
                match apps {
                    Ok(apps) => {
                        for mut app in apps {
                            let name = app.name.clone();
                            // build full path to the runtimefs
                            app.runtimefs = format!("{}{}",ctr_config.get_runtimefs_base(), app.runtimefs);
                            // build full path to the appfs
                            app.appfs = format!("{}{}",ctr_config.get_appfs_base(), app.appfs);
                            // build full path to base snapshot
                            app.load_dir = app.load_dir.map(|s| format!("{}{}",ctr_config.get_snapshot_base(), s));
                            // build full paths to diff snapshots: comma-separated list
                            app.diff_dirs = app.diff_dirs.map(|s| 
                                s.split(',').collect::<Vec<&str>>().iter()
                                    .map(|s| format!("{}diff/{}",ctr_config.get_snapshot_base(), s))
                                    .collect::<Vec<String>>().join(",")
                            );
                            // TODO: currently all apps use the same kernel
                            app.kernel = Url::parse(&ctr_config.kernel_path)
                                .expect("Bad kernel path URL").path().to_string();
                            // use `firerunner`'s default DEFAULT_KERNEL_CMDLINE
                            // defined in firecracker/vmm/lib.rs
                            app.cmdline = None;
                            // `snapctr` does not support generate snapshots
                            app.dump_dir = None;

                            function_configs.insert(name.clone(), app);
                            idle.insert(name.clone(), VmList::new());
                        }
                        // set default total memory to free memory on the machine
                        let total_mem = get_machine_memory();
                        return Some(Controller {
                            controller_config: ctr_config,
                            function_configs: function_configs,
                            idle: idle,
                            total_num_vms: AtomicUsize::new(0),
                            total_mem,
                            free_mem: AtomicUsize::new(total_mem),
                        });
                    }
                    Err(e) => error!("serde_yaml failed to parse function config file: {:?}", e)
                }
                return None;
            }
            Err(e) => {
                error!("function config file failed to open: {:?}", e);
                return None;
            }
        }
    }

    /// Shutdown the controller
    /// Things needed to do for shutdown:
    /// 1. Go through idle lists and shutdown all vms
    pub fn shutdown(&self) {
        for key in self.idle.keys() {
            let vmlist = self.idle.get(key).unwrap();
            vmlist.list.lock().map(|mut l| {
                for vm in l.iter_mut() {
                    vm.shutdown();
                }
            }).expect("poisoned lock");
        }
    }

    /// Try to find an idle vm from the function's idle list
    pub fn get_idle_vm(&self, function_name: &str) -> Result<Vm, Error> {
        if let Some(idle_list) = self.idle.get(function_name) {
            return idle_list.pop().ok_or(Error::NoIdleVm);
        }
        return Err(Error::FunctionNotExist);
    }

    /// Push the vm onto its function's idle list
    pub fn release(&self, function_name: &str, vm: Vm) {
        self.idle.get(function_name).unwrap().push(vm); // unwrap should always work
    }

    /// Try to allocate and launch a new vm for a function
    /// allocate() first checks if there's enough free resources by looking at `free_mem`. If there
    /// is, it proactively "reserve" requisite memory by decrementing `free_mem`.
    ///
    /// Allocation can fail under 2 conditions:
    /// 1. when there's not enough resources on the machine
    ///    (Err(Error::LowMemory))
    /// 2. launching the vm process failed (i.e., Vm::new())
    ///    (Err(Error::StartVm(vm::Error)))
    pub fn allocate(
        &self,
        function_config: &FunctionConfig,
        vm_listener: &UnixListener,
        cid: u32,
        network: &str,
    ) -> Result<Vm, Error> {
        match self.free_mem.fetch_update(
            Ordering::SeqCst,
            Ordering::SeqCst,
            |x| match x >= function_config.memory {
                true => Some(x - function_config.memory),
                false => None,
            },
        ) {
            Ok(_) => {
                let id = self.total_num_vms.fetch_add(1, Ordering::Relaxed);
                trace!("Allocating new VM. ID: {:?}, App: {:?}", id, function_config.name);

                return Vm::new(&id.to_string(), function_config, vm_listener, cid, Some(network))
                       .map_err(|e| {
                    // Make sure to "unreserve" the resource by incrementing
                    // `Controller::free_mem`
                    self.free_mem.fetch_add(function_config.memory, Ordering::Relaxed);
                    Error::StartVm(e)
                });
            }
            Err(free_mem) => {
                return Err(Error::LowMemory(free_mem));
            }
        }
    }

    /// Evict one or more vms to free `mem` MB of memory
    pub fn evict(&self, mem: usize) -> bool {
        let mut freed: usize = 0;
        let start = Instant::now();

        while freed < mem && start.elapsed() < EVICTION_TIMEOUT {
            for key in self.idle.keys() {
                let vmlist = self.idle.get(key).unwrap();

                // instead of evicting from the first non-empty list in the map,
                // collect some function popularity data and evict based on that.
                // This is where some policies can be implemented.
                if let Some(mut vm) = vmlist.try_pop() {
                    vm.shutdown();
                    self.free_mem.fetch_add(vm.memory, Ordering::Relaxed);
                    freed = freed + vm.memory;
                }
            }
        }

        if freed >= mem {
            return true;
        }

        return false;
    }

    /// Go through all the idle lists and remove a VM from the first non-empty
    /// idle list that we encounter
    pub fn find_evict_candidate(&self, function_name: &str) -> Result<Vm, Error> {
        for key in self.idle.keys() {
            if key == function_name {
                continue;
            }
            if let Some(vm) = self.idle.get(key).expect("key doesn't exist").try_pop() {
                return Ok(vm);
            }
        }
        return Err(Error::InsufficientEvict);
    }

    //pub fn evict_and_allocate(&self, mem: usize, function_config: &FunctionConfig) -> Result<Vm, Error> {
    //    if !self.evict(mem) {
    //        return Err(Error::InsufficientEvict);
    //    }
    //    return self.allocate(function_config);
    //}

    pub fn get_function_config(&self, function_name: &str) -> Option<&FunctionConfig> {
        self.function_configs.get(function_name)
    }

    pub fn get_function_memory(&self, function_name: &str) -> Option<usize> {
        self.function_configs.get(function_name).map(|f| f.memory)
    }

    /// should only be called once before Vms are launch. Not supporting
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
        self.free_mem = AtomicUsize::new(mem);
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

// check concurrent allocate() correctly decrements Controller.free_mem
// check vms are correctly pushed concurrently to idle lists after use with release()
// check get_idle_vm() returns unique vms to concurrent calls
#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to create a Controller value with `mem` amount of memory
    /// If `mem` is set to 0, the amount of memory is set to the total memory
    /// of the machine.
    fn build_controller(mem: usize) -> Controller {
        let ctr_config = ControllerConfig {
            kernel_path: "".to_string(),
            kernel_boot_args: "".to_string(),
            runtimefs_dir: "".to_string(),
            appfs_dir: "".to_string(),
            function_config: "file://localhost/etc/snapfaas/example_function_configs.yaml"
                .to_string(),
        };

        let mut ctr = Controller::new(ctr_config).unwrap();
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
        let lp_config = controller.get_function_config("lorempy2").unwrap();
        let total_mem = controller.total_mem;
        let num_vms = 100;

        for i in 0..num_vms {
            controller.allocate(lp_config);
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
        let num_vms = 123;

        let sctr = Arc::new(controller);

        let mut handles = vec![];

        for i in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                let c = ctr.get_function_config("lorempy2").unwrap();
                ctr.allocate(c);
            });
            handles.push(h);
        }

        for h in handles {
            h.join();
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
        let lp_config = controller.get_function_config("lorempy2").unwrap();

        for i in 0..8 {
            if let None = controller.allocate(lp_config) {
                panic!("allocate failed before exhausting resources");
            }
        }

        if let Some(vm) = controller.allocate(lp_config) {
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
        let num_vms = total_mem / sctr.get_function_config("lorempy2").unwrap().memory;

        let mut handles = vec![];

        for i in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                let c = ctr.get_function_config("lorempy2").unwrap();
                if let None = ctr.allocate(c) {
                    panic!("allocate failed before exhausting resources");
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join();
        }

        let c = sctr.get_function_config("lorempy2").unwrap();
        for i in 0..num_vms {
            if let Some(vm) = sctr.allocate(c) {
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
        let lp_config = controller.get_function_config("lorempy2").unwrap();

        assert_eq!(
            controller
                .idle
                .get(&lp_config.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            controller
                .idle
                .get(&lp_config.name)
                .unwrap()
                .list
                .lock()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(controller.free_mem.load(Ordering::Relaxed), 1024);

        for _ in 0..8 {
            match controller.allocate(lp_config) {
                Some(vm) => controller.release(&lp_config.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            controller
                .idle
                .get(&lp_config.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            8
        );
        assert_eq!(
            controller
                .idle
                .get(&lp_config.name)
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
        let c = sctr.get_function_config("lorempy2").unwrap();
        let num_vms = total_mem / c.memory;

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );
        assert_eq!(sctr.free_mem.load(Ordering::Relaxed), total_mem);

        let mut handles = vec![];

        for i in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                let c = ctr.get_function_config("lorempy2").unwrap();
                match ctr.allocate(c) {
                    Some(vm) => ctr.release(&c.name, vm),
                    None => panic!("allocate failed before exhausting resources"),
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join();
        }

        let c = sctr.get_function_config("lorempy2").unwrap();
        for i in 0..num_vms {
            if let Some(vm) = sctr.allocate(c) {
                panic!("allocate succeeds after exhausting resources");
            }
        }
        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
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
        let c = sctr.get_function_config("lorempy2").unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(c) {
                Some(vm) => sctr.release(&c.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            num_vms
        );

        for _ in 0..num_vms {
            if let None = sctr.get_idle_vm(&c.name) {
                panic!("idle list should not be empty")
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Some(vm) = sctr.get_idle_vm(&c.name) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
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
        let c = sctr.get_function_config("lorempy2").unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(c) {
                Some(vm) => sctr.release(&c.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            num_vms
        );

        let mut handles = vec![];

        for _ in 0..num_vms {
            let ctr = sctr.clone();
            let h = thread::spawn(move || {
                let c = ctr.get_function_config("lorempy2").unwrap();
                if let None = ctr.get_idle_vm(&c.name) {
                    panic!("idle list should not be empty")
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join();
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Some(vm) = sctr.get_idle_vm(&c.name) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );
    }

    #[test]
    /// evict should remove a vm from its idle list and increment `free_mem`.
    fn test_eviction_single_vm() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config("lorempy2").unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(c) {
                Some(vm) => sctr.release(&c.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            sctr.total_mem - 128 * num_vms
        );

        for i in 0..num_vms {
            if !sctr.evict(c.memory) {
                panic!("idle list should not be empty")
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Some(vm) = sctr.get_idle_vm(&c.name) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );
    }

    #[test]
    /// evict should remove a vm from its idle list and increment `free_mem`.
    fn test_eviction_single_vm_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config("lorempy2").unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(c) {
                Some(vm) => sctr.release(&c.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
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
                let c = ctr.get_function_config("lorempy2").unwrap();
                if !ctr.evict(c.memory) {
                    panic!("idle list should not be empty")
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join();
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );

        if let Some(vm) = sctr.get_idle_vm(&c.name) {
            panic!("idle list should be empty");
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
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
        let c = sctr.get_function_config("lorempy2").unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(c) {
                Some(vm) => sctr.release(&c.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            num_vms
        );
        assert_eq!(
            sctr.free_mem.load(Ordering::Relaxed),
            sctr.total_mem - c.memory * num_vms
        );

        let num_evict = num_vms / 2;

        for i in 0..num_evict {
            if !sctr.evict(c.memory * 2) {
                panic!("idle list should not be empty")
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms - num_evict * 2
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            num_vms - num_evict * 2
        );
    }

    #[test]
    fn test_eviction_multi_vms_concurrent() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config("lorempy2").unwrap();
        let num_vms = total_mem / c.memory;

        for _ in 0..num_vms {
            match sctr.allocate(c) {
                Some(vm) => sctr.release(&c.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
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
                let c = ctr.get_function_config("lorempy2").unwrap();
                if !ctr.evict(c.memory * 2) {
                    panic!("idle list should not be empty")
                }
            });
            handles.push(h);
        }

        for h in handles {
            h.join();
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms - num_evict * 2
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            num_vms - num_evict * 2
        );
    }

    #[test]
    /// evict should fail and return if there's nothing idle to evict
    fn test_eviction_failure_not_block() {
        let controller = build_controller(0);
        let total_mem = controller.total_mem;
        let num_vms = 1;

        let sctr = Arc::new(controller);
        let c = sctr.get_function_config("lorempy2").unwrap();

        for _ in 0..num_vms {
            match sctr.allocate(c) {
                Some(vm) => sctr.release(&c.name, vm),
                None => panic!("allocate failed before exhausting resources"),
            }
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            num_vms
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
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
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );

        if sctr.evict(c.memory) {
            panic!("eviction should fail")
        }

        assert_eq!(
            sctr.idle
                .get(&c.name)
                .unwrap()
                .num_vms
                .load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            sctr.idle.get(&c.name).unwrap().list.lock().unwrap().len(),
            0
        );
    }
}
