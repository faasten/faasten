use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::collections::{HashMap, BTreeMap};
use std::fs::File;
use url::Url;

use crate::configs::{ControllerConfig, FunctionConfig};
use crate::*;

use log::{error};
use serde_yaml;

#[derive(Debug)]
struct Vm {
    id: u32,
}

#[derive(Debug)]
struct VmList {
    num_vms: AtomicUsize,
    list: Mutex<Vec<Vm>>,
}

#[derive(Debug)]
pub struct Controller {
    controller_config: ControllerConfig,
    function_configs: BTreeMap<String, FunctionConfig>,
    idle: HashMap<String, VmList>,
}

impl Controller {
    pub fn new(ctr_config: ControllerConfig) -> Option<Controller> {
        let mut function_configs = BTreeMap::<String, FunctionConfig>::new();
        let mut idle = HashMap::<String, VmList>::new();
        match open_url(&ctr_config.function_config) {
            Ok(fd) => {
                let apps: serde_yaml::Result<Vec<FunctionConfig>> = serde_yaml::from_reader(fd);
                if let Ok(apps) = apps {
                    // populate function_configs map
                    // Create idle list
                    for app in apps {
                        let name = app.name.clone();
                        function_configs.insert(name.clone(), app);
                        idle.insert(name.clone(), VmList::new());
                    }
                    return Some(Controller {
                        controller_config: ctr_config,
                        function_configs: function_configs,
                        idle: idle,
                    });
                }
            }
            Err(e) => error!("function config file failed to open: {:?}", e),
        }

        error!("serde_yaml failed to parse function config file");
        return None;
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
