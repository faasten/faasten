use std::path::PathBuf;
#[cfg(not(test))]
use std::process::{Child, Command, Stdio};
#[cfg(not(test))]
use std::net::Shutdown;
#[cfg(not(test))]
use std::os::unix::net::{UnixStream, UnixListener};
#[cfg(not(test))]
use std::time::Instant;
#[cfg(not(test))]
use log::{info, error};

#[cfg(not(test))]
use crate::configs::FunctionConfig;
#[cfg(not(test))]
use crate::{request};
use crate::request::Request;

#[derive(Debug)]
pub enum Error {
    ProcessSpawn(std::io::Error),
    //VmTimeout(RecvTimeoutError),
    VmWrite(std::io::Error),
    VmRead(std::io::Error),
    KernelNotExist,
    RootfsNotExist,
    AppfsNotExist,
    LoadDirNotExist,
    NotString(std::string::FromUtf8Error),
}

#[derive(Debug)]
pub struct VmAppConfig {
    pub rootfs: String,
    pub appfs: String,
    pub load_dir: Option<PathBuf>,
    pub dump_dir: Option<PathBuf>,
}

#[derive(Debug)]
#[cfg(not(test))]
pub struct Vm {
    pub id: usize,
    pub memory: usize, // MB
    pub function_name: String,
    //vsock_stream: VsockStream,
    conn: UnixStream,
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

#[cfg(not(test))]
impl Vm {
    /// Launch a vm instance and return a Vm value
    /// When this function returns, the VM has finished booting and is ready
    /// to accept requests.
    pub fn new(
        id: &str,
        function_config: &FunctionConfig,
        vm_listener: &UnixListener,
        cid: u32,
        network: Option<&str>,
        firerunner: &str,
        force_exit: bool,
    ) -> Result<(Vm, Vec<Instant>), Error> {
        let mut ts_vec = Vec::with_capacity(10);
        ts_vec.push(Instant::now());
        let mut cmd = Command::new(firerunner);
        let mem_str = function_config.memory.to_string();
        let vcpu_str = function_config.vcpus.to_string();
        let cid_str = cid.to_string();
        let mut args = vec![
                "--id",
                id,
                "--kernel",
                &function_config.kernel,
                "--mem_size",
                &mem_str,
                "--vcpu_count",
                &vcpu_str,
                "--rootfs",
                &function_config.runtimefs,
                "--cid",
                &cid_str,
        ];

        if function_config.appfs != "" {
            args.extend_from_slice(&["--appfs", &function_config.appfs]);
        }
        if let Some(load_dir) = function_config.load_dir.as_ref() {
            args.extend_from_slice(&["--load_from", &load_dir]);
        }
        if let Some(dump_dir) = function_config.dump_dir.as_ref() {
            args.extend_from_slice(&["--dump_to", &dump_dir]);
        }
        if let Some(diff_dirs) = function_config.diff_dirs.as_ref() {
            args.extend_from_slice(&["--diff_dirs", &diff_dirs]);
        }
        if let Some(cmdline) = function_config.cmdline.as_ref() {
            args.extend_from_slice(&["--kernel_args", &cmdline]);
        }
        if function_config.copy_base {
            args.push("--copy_base");
        }
        if function_config.copy_diff {
            args.push("--copy_diff");
        }

        // network config should be of the format <TAP-Name>/<MAC Address>
        if let Some(network) = network {
            let v: Vec<&str> = network.split('/').collect();
            args.extend_from_slice(&["--tap_name", v[0]]);
            args.extend_from_slice(&["--mac", v[1]]);
        }

        info!("args: {:?}", args);
        let cmd = cmd.args(args);
        ts_vec.push(Instant::now());
        let mut vm_process: Child = cmd.stdin(Stdio::null())
            .spawn()
            .map_err(|e| Error::ProcessSpawn(e))?;
        ts_vec.push(Instant::now());

        if force_exit {
            vm_process.wait().unwrap();
            crate::unlink_unix_sockets();
            std::process::exit(0);
        }

        let (conn, _) = vm_listener.accept().unwrap();
        ts_vec.push(Instant::now());

        return Ok((Vm {
            id: id.parse::<usize>().expect("vm id not int"),
            function_name: function_config.name.clone(),
            memory: function_config.memory,
            conn,
            process: vm_process,
        }, ts_vec));
    }

    /// Send request to vm and wait for its response
    pub fn process_req(&mut self, req: Request) -> Result<String, Error> {
        let req_str = req.payload_as_string();

        let buf = req_str.as_bytes();

        request::write_u8_vm(&buf, &mut self.conn).map_err(|e| {
            Error::VmWrite(e)
        })?;

        // This is a blocking call
        // TODO: add a timeout for this read (maybe ~15 minutes)
        match request::read_u8_vm(&mut self.conn) {
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
        if let Err(e) = self.conn.shutdown(Shutdown::Both) {
            error!("Failed to shut down unix connection: {:?}", e);
        }
        if let Err(e) = self.process.kill() {
            error!("VM already exited: {:?}", e);
        }
    }
}

#[derive(Debug)]
#[cfg(test)]
pub struct Vm {
    pub id: usize,
    pub memory: usize, // MB
}

#[cfg(test)]
impl Vm {
    /// Create a dummy VM for controller tests
    pub fn new_dummy(memory: usize) -> Self {
        Vm {
            id: 0,
            memory,
        }
    }

    pub fn process_req(&mut self, _: Request) -> Result<String, Error> {
        Ok(String::new())
    }

    pub fn shutdown(&mut self) {
    }
}
