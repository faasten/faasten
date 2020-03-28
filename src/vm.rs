use nix::unistd::Pid;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use crate::configs::FunctionConfig;
use crate::request::Request;
use crate::request;
use cgroups::{cgroup_builder::CgroupBuilder, Cgroup};

pub enum VmStatus{
    NotReady,
    ReadyToReceive = 1,
    Unresponsive,
    Crashed,
}

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
    pub function_name: String,
    process: Child,
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
    /// Launch a vm instance and return a Vm value
    /// When this function returns, the VM has finished booting and is ready
    /// to accept requests.
    pub fn new(id: usize, function_config: &FunctionConfig) -> Option<Vm> {
        let mut vm_process = Command::new("target/release/firerunner")
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
        let mut ready_msg = vec![0;1];
        {
            let stdout = vm_process.stdout.as_mut().unwrap();
            //stdout.read_to_string(&mut ready_msg);
            stdout.read(&mut ready_msg);
        }

        return Some(Vm {
            id: id,
            memory: function_config.memory,
            process: vm_process,
            function_name: function_config.name.clone(),
        });
    }

    /// Send request to vm and wait for its response
    pub fn process_req(&mut self, req: Request) -> Result<String, String> {
        let req_str = req.to_string().unwrap();
        let mut req_sender = self.process.stdin.as_mut().unwrap();

        let buf = req_str.as_bytes();

        request::write_u8_vm(&buf, &mut req_sender);

        let rsp_buf = request::read_u8_vm(&mut self.process.stdout.as_mut().unwrap()).unwrap();
        return Ok(String::from_utf8(rsp_buf).unwrap());
    }

    /// shutdown this vm
    pub fn shutdown(&mut self) {
        // TODO: not sure if kill() waits for the child process to terminate
        // before returning. This is relevant for shutdown latency measurement.
        // TODO: std::process::Child.kill() is equivalent to sending a SIGKILL
        // on unix platforms which means the child process won't be able to run
        // its clean up process. Previously, we shutdown VMs through SIGTERM
        // which does allow a shutdown process. We need to make sure using
        // SIGKILL won't create any issues with vms.
        self.process.kill();
    }
}
