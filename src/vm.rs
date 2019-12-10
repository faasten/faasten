use nix::unistd::Pid;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use crate::configs::FunctionConfig;
use crate::request::Request;
use cgroups::{cgroup_builder::CgroupBuilder, Cgroup};

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
    pub memory: usize, // MB
    pub process: Child,
    /*
    pub process: Pid,
    cgroup_name: PathBuf,
    pub cpu_share: usize,
    pub vcpu_count: usize,
    pub kernel: String,
    pub kernel_args: String,
    pub ready_notifier: File, // Vm writes to this File when setup finishes

    pub app_config: VmAppConfig,
    */
}

impl Vm {
    /// start a vm instance and return a Vm value
    pub fn new(id: usize, function_config: &FunctionConfig) -> Option<Vm> {
        let mut vm_process = Command::new("/etc/snapfaas/firerunner")
            .args(&[
                "--id",
                &id.to_string(),
                "--kernel",
                "/ect/snapfaas/vmlinux",
                "--kernel_args",
                "quiet",
                "--mem_size",
                &function_config.memory.to_string(),
                "--vcpu_count",
                &function_config.vcpus.to_string(),
                "--rootfs",
                &function_config.runtimefs,
                "--appfs",
                &function_config.appfs,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn();

        if vm_process.is_err() {
            return None;
        }
        let mut vm_process = vm_process.unwrap();

        //let mut ready_msg = String::new();
        let mut ready_msg = vec![0;16];
        {
            let stdout = vm_process.stdout.as_mut().unwrap();
            //stdout.read_to_string(&mut ready_msg);
            stdout.read(&mut ready_msg);
        }

        return Some(Vm {
            id: id,
            memory: function_config.memory,
            process: vm_process,
        });
    }

    /// Send request to vm and wait for its response
    pub fn process_req(&mut self, req: Request) -> Result<String, String> {
        match self
            .process
            .stdin
            .as_mut()
            .unwrap()
            .write_all(req.to_string().unwrap().as_bytes())
        {
            Ok(_) => (),
            Err(_) => return Err(String::from("Request failed to send")),
        }

        let mut response = vec![0;96];
        {
            let stdout = self.process.stdout.as_mut().unwrap();
            //stdout.read_to_string(&mut ready_msg);
            stdout.read(&mut response);
        }

        return Ok(String::from_utf8(response).unwrap());
    }

    pub fn shutdown(&mut self) {
        self.process.kill();
    }
}
