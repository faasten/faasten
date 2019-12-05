use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::collections::{HashMap, BTreeMap};
use std::fs::File;
use url::Url;

use crate::configs::{ControllerConfig, FunctionConfig};
use crate::*;
use crate::vm::Vm;

use log::{error};
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

    pub fn get_idle_vm(&self, function_name: &str) -> Option<Vm> {
        if let Some(idle) = self.idle.get(function_name) {
            return idle.pop();
        }
        return None;
    }

    pub fn allocate(&self, function_name: &str) -> Option<Vm> {
        let id = self.total_num_vms.fetch_add(1, Ordering::Relaxed);
        return Some(Vm {
            id: id,
        });
    }

    pub fn evict(&self, mem: usize) -> bool {
        return false;
    }

    pub fn evict_and_allocate(&self, mem: usize, function_name: &str) -> Option<Vm> {
        if !self.evict(mem) {
            return None;
        }
        return self.allocate(function_name);
    }

    pub fn get_function_config(&self, function_name: &str) -> Option<FunctionConfig> {
        self.function_configs.get(function_name).cloned()
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
        self.list.lock().expect("poisoned lock on idle list").push(val);
        self.num_vms.fetch_add(1, Ordering::Relaxed);
    }
}
