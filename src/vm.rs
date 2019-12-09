use std::path::PathBuf;
use std::fs::File;
use nix::unistd::{Pid};

use cgroups::{Cgroup, cgroup_builder::CgroupBuilder};
use crate::request::Request;
use crate::configs::FunctionConfig;

#[derive(Debug)]
pub struct VmAppConfig {
    pub rootfs: String,
    pub appfs: String,
    pub load_dir: Option<PathBuf>,
    pub dump_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Vm {
    pub id: usize,
    pub process: Pid,
    cgroup_name: PathBuf,
    pub memory: usize, // MB
    pub cpu_share: usize,
    pub vcpu_count: usize,
    pub kernel: String,
    pub kernel_args: String,
    pub ready_notifier: File, // Vm writes to this File when setup finishes

    pub app_config: VmAppConfig,
}

impl Vm {
    /// start a vm instance and return a Vm value
    pub fn new(id: usize, function_config: &FunctionConfig) -> Vm {
        Vm {
            id: id,
            memory: function_config.memory,
        }
    }

    pub fn process_req(&self, req: Request) -> Result<String, String> {
        return Ok(String::from("success"));
    }

    pub fn shutdown(&self) {
    }
}
