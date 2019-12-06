use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use url::Url;

use crate::configs::{ControllerConfig, FunctionConfig};
use crate::vm::Vm;
use crate::*;

use log::error;
use serde_yaml;

#[derive(Debug)]
pub struct VmList {
    num_vms: AtomicUsize,
    list: Mutex<Vec<Vm>>,
}

#[derive(Debug)]
pub struct Controller {
    controller_config: ControllerConfig,
    function_configs: BTreeMap<String, FunctionConfig>,
    idle: HashMap<String, VmList>,
    total_num_vms: AtomicUsize,
    total_mem: usize,
    free_mem: AtomicUsize,
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
                if let Ok(apps) = apps {
                    for app in apps {
                        let name = app.name.clone();
                        function_configs.insert(name.clone(), app);
                        idle.insert(name.clone(), VmList::new());
                    }
                    return Some(Controller {
                        controller_config: ctr_config,
                        function_configs: function_configs,
                        idle: idle,
                        total_num_vms: AtomicUsize::new(0),
                        total_mem: get_machine_memory(),
                        free_mem: AtomicUsize::new(get_machine_memory()),
                    });
                }
                error!("serde_yaml failed to parse function config file");
                return None;
            }
            Err(e) => {
                error!("function config file failed to open: {:?}", e);
                return None;
            }
        }
    }

    /// Push the vm onto its function's idle list
    pub fn release(&self, function_name: &str, vm: Vm) {
        self.idle.get(function_name).unwrap().push(vm); // unwrap should always work
    }

    /// Try to find an idle vm from the function's idle list
    pub fn get_idle_vm(&self, function_name: &str) -> Option<Vm> {
        if let Some(idle) = self.idle.get(function_name) {
            return idle.pop();
        }
        return None;
    }

    /// Try to allocate and launch a new vm for a function
    /// Allocation can fail when there's not enough resources on the machine.
    pub fn allocate(&self, function_config: &FunctionConfig) -> Option<Vm> {
        match self.free_mem.fetch_update(
            |x| match x >= function_config.memory {
                true => Some(x - function_config.memory),
                false => None,
            },
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => {
                let id = self.total_num_vms.fetch_add(1, Ordering::Relaxed);
                return Some(Vm::new(id));
            }
            Err(_) => {
                return None;
            }
        }
    }

    /// Evict one or more vms to free `mem` MB of memory
    pub fn evict(&self, mem: usize) -> bool {
        let mut freed: usize = 0;
        let mut passes = 0;
        while freed < mem && passes < 3 {
            for key in self.idle.keys() {
                let vmlist = self.idle.get(key).unwrap();

                // instead of evicting from the first non-empty list in the map,
                // collect some function popularity data and evict based on that.
                // This is where some policies can be implemented.
                if let Ok(mut mutex) = vmlist.list.try_lock() {
                    if let Some(vm) = mutex.pop() {
                        vm.shutdown();
                        self.free_mem.fetch_add(vm.memory, Ordering::Relaxed);
                        freed = freed + vm.memory;
                    }
                }
            }
            passes = passes + 1;
        }

        return false;
    }

    pub fn evict_and_allocate(&self, mem: usize, function_config: &FunctionConfig) -> Option<Vm> {
        if !self.evict(mem) {
            return None;
        }
        return self.allocate(function_config);
    }

    pub fn get_function_config(&self, function_name: &str) -> Option<&FunctionConfig> {
        self.function_configs.get(function_name)
    }

    pub fn get_function_memory(&self, function_name: &str) -> Option<usize> {
        self.function_configs.get(function_name).map(|f| f.memory)
    }

    /// should only be called once before Vms are launch. Not supporting
    /// changing total available memory on the fly.
    pub fn set_total_mem(&mut self, mem: usize) {
        if mem > 0 && mem < get_machine_memory() {
            self.total_mem = mem;
            self.free_mem = AtomicUsize::new(mem);
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

    pub fn pop(&self) -> Option<Vm> {
        match self.list.lock().expect("poisoned lock on idle list").pop() {
            Some(v) => {
                self.num_vms.fetch_sub(1, Ordering::Relaxed);
                return Some(v);
            }
            None => return None,
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
