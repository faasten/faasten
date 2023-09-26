use serde::{Deserialize, Serialize};

use crate::configs::FunctionConfig;

#[derive(Default, Clone, Debug, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub struct Function {
    // TODO support snapshots
    pub memory: usize,
    pub app_image: String,
    pub runtime_image: String,
    pub kernel: String,
}

// used by singlevm. singlevm allows more complicated configurations than multivm.
impl From<FunctionConfig> for Function {
    fn from(cfg: FunctionConfig) -> Self {
        Self {
            memory: cfg.memory,
            app_image: cfg.appfs.unwrap_or_default(),
            runtime_image: cfg.runtimefs,
            kernel: cfg.kernel,
        }
    }
}

impl From<crate::sched::message::Function> for Function {
    fn from(pbf: crate::sched::message::Function) -> Self {
        Self {
            memory: pbf.memory as usize,
            app_image: pbf.app_image,
            runtime_image: pbf.runtime,
            kernel: pbf.kernel,
        }
    }
}

impl From<Function> for crate::sched::message::Function {
    fn from(f: Function) -> Self {
        Self {
            memory: f.memory as u64,
            app_image: f.app_image,
            runtime: f.runtime_image,
            kernel: f.kernel,
        }
    }
}
