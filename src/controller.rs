use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::collections::{HashMap, BTreeMap};

use crate::configs::{ControllerConfig, FunctionConfig};

struct Vm {
    id: u32,
}

struct VmList {
    num_vms: AtomicUsize,
    list: Mutex<Vec<Vm>>,
}

struct Controller {
    controller_config: ControllerConfig,
    function_configs: BTreeMap<String, FunctionConfig>,
    idle: HashMap<String, VmList>,
}

impl Controller {
    pub fn new(config: ControllerConfig) {

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
        match self.list.lock().unwrap().pop() {
            Some(v) => {
                self.num_vms.fetch_sub(1, Ordering::Relaxed);
                return Some(v);
            }
            None => return None,
        }
    }

    pub fn push(&self, val: Vm) {
        self.list.lock().unwrap().push(val)
    }
}
