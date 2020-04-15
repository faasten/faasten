use nix::unistd::Pid;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use crate::configs::FunctionConfig;
use crate::request::Request;
use crate::request;
use cgroups::{cgroup_builder::CgroupBuilder, Cgroup};
use log::{info, trace, warn, error};

#[derive(Debug)]
pub enum VmStatus{
    NotReady,
    Ready = 65,
    KernelNotExist,
    RootfsNotExist,
    AppfsNotExist,
    LoadDirNotExist,
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

#[derive(Debug)]
pub enum Error {
    ProcessSpawn(std::io::Error),
    ReadySignal(std::io::Error),
    VmWrite(std::io::Error),
    VmRead(std::io::Error),
    KernelNotExist,
    RootfsNotExist,
    AppfsNotExist,
    LoadDirNotExist,
    NotString(std::string::FromUtf8Error),
}

impl Vm {
    /// Launch a vm instance and return a Vm value
    /// When this function returns, the VM has finished booting and is ready
    /// to accept requests.
    pub fn new(id: &str, function_config: &FunctionConfig) -> Result<Vm, Error> {
        let mut cmd = Command::new("target/release/firerunner");
        let mut cmd = if function_config.load_dir.is_none() {
            cmd
            .args(&[
                "--id",
                id,
                "--kernel",
                "/etc/snapfaas/vmlinux",
                "--mem_size",
                &function_config.memory.to_string(),
                "--vcpu_count",
                &function_config.vcpus.to_string(),
                "--rootfs",
                &function_config.runtimefs,
                "--appfs",
                &function_config.appfs,
            ])
        } else {
            cmd
            .args(&[
                "--id",
                id,
                "--kernel",
                "/etc/snapfaas/vmlinux",
                "--mem_size",
                &function_config.memory.to_string(),
                "--vcpu_count",
                &function_config.vcpus.to_string(),
                "--rootfs",
                &function_config.runtimefs,
                "--appfs",
                &function_config.appfs,
                "--load_from",
                &function_config.load_dir.as_ref().unwrap(),
            ])

        };
        let mut vm_process: Child = cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| Error::ProcessSpawn(e))?;

        //let mut ready_msg = String::new();
        let mut ready_msg = vec![0;1];
        {
            let stdout = vm_process.stdout.as_mut().unwrap();
            // If no ready message is received, kill the child process and
            // return None.
            // TODO: have a timeout here in case the firerunner process does
            // not die but hangs
            match stdout.read(&mut ready_msg) {
                Ok(_) => {
                    // check that the ready_msg is the number 42
                    let code = ready_msg[0] as u32;
                    if code != VmStatus::Ready as u32 {
                        vm_process.kill();
                        let err = match code {
                            // TODO: need a better way to convert int to enum
                            66 =>Error::KernelNotExist,
                            67 =>Error::RootfsNotExist,
                            68 =>Error::AppfsNotExist,
                            69 =>Error::LoadDirNotExist,
                            _ => Error::ReadySignal(io::Error::new(io::ErrorKind::InvalidData, code.to_string())),
                        };
                        return Err(err);
                    }
                },
                Err(e) => {
                    vm_process.kill();
                    return Err(Error::ReadySignal(e));
                }
            }
        }

        return Ok(Vm {
            id: id.parse::<usize>().expect("vm id not int"),
            function_name: function_config.name.clone(),
            memory: function_config.memory,
            process: vm_process,
        });
    }

    /// Send request to vm and wait for its response
    pub fn process_req(&mut self, req: Request) -> Result<String, Error> {
        let req_str = req.payload_as_string();
        let mut req_sender = self.process.stdin.as_mut().unwrap();

        let buf = req_str.as_bytes();

        request::write_u8_vm(&buf, &mut req_sender).map_err(|e| {
            Error::VmWrite(e)
        })?;

        // This is a blocking call
        // TODO: add a timeout for this read (maybe ~15 minutes)
        match request::read_u8_vm(&mut self.process.stdout.as_mut().unwrap()) {
            Ok(rsp_buf) => {
                String::from_utf8(rsp_buf).map_err(|e| {
                    Error::NotString(e)
                })
            }
            Err(e) => {
                Err(Error::VmRead(e))
            }
        }
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
