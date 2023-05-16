//! Host-side VM handle that transfer data in and out of the VM through VSOCK socket and
//! implements syscall API

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::process::Stdio;
use std::string::String;

use labeled::buckle::Buckle;
use log::{debug, error};
use prost::Message;
use tokio::process::{Child, Command};

use crate::configs::FunctionConfig;
use crate::syscall_server::{SyscallChannel, SyscallChannelError};
use crate::syscalls;
use crate::syscalls::syscall::Syscall as SC;

//const MACPREFIX: &str = "AA:BB:CC:DD";

#[derive(Debug)]
pub enum Error {
    ProcessSpawn(std::io::Error),
    Rpc(prost::DecodeError),
    VsockListen(std::io::Error),
    VsockWrite(std::io::Error),
    VsockRead(std::io::Error),
    HttpReq(reqwest::Error),
    AuthTokenInvalid,
    AuthTokenNotExist,
    KernelNotExist,
    RootfsNotExist,
    AppfsNotExist,
    LoadDirNotExist,
    DB(lmdb::Error),
    BlobError(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::BlobError(e)
    }
}

/// Specify the `O_DIRECT` flag when open a disk image which is a regular file
pub struct OdirectOption {
    pub base: bool,
    pub diff: bool,
    pub rootfs: bool,
    pub appfs: bool,
}

#[derive(Debug)]
pub struct VmHandle {
    conn: UnixStream,
    #[allow(dead_code)]
    // This field is never used, but we need to it make sure the Child isn't dropped and, thus,
    // killed, before the VmHandle is dropped.
    vm_process: Child,
}

#[derive(Debug)]
pub struct Vm {
    pub id: usize,
    pub function: super::fs::Function,
    pub label: Buckle,
    pub handle: Option<VmHandle>,
}

impl Vm {
    pub fn new(id: usize, function: super::fs::Function) -> Self {
        Self {
            id,
            function,
            label: Buckle::public(),
            handle: None,
        }
    }

    /// Launch the current Vm instance.
    /// When this function returns, the VM has finished booting and is ready to accept requests.
    pub fn launch(
        &mut self,
        vm_listener: std::os::unix::net::UnixListener,
        cid: u32,
        force_exit: bool,
        function_config: FunctionConfig,
        odirect: Option<OdirectOption>,
    ) -> Result<(), Error> {
        if self.handle.is_some() {
            return Ok(());
        }
        let mem_str = function_config.memory.to_string();
        let vcpu_str = function_config.vcpus.to_string();
        let cid_str = cid.to_string();
        let id_str = self.id.to_string();
        let mut args = vec![
            "--id",
            &id_str,
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

        if let Some(f) = function_config.appfs.as_ref() {
            args.extend_from_slice(&["--appfs", f]);
        }
        if let Some(load_dir) = function_config.load_dir.as_ref() {
            args.extend_from_slice(&["--load_from", load_dir]);
            if function_config.copy_base {
                args.push("--copy_base");
            }
            if function_config.copy_diff {
                args.push("--copy_diff");
            }
            if function_config.load_ws {
                args.push("--load_ws");
            }
        }
        if let Some(dump_dir) = function_config.dump_dir.as_ref() {
            args.extend_from_slice(&["--dump_to", dump_dir]);
            if function_config.dump_ws {
                args.push("--dump_ws");
            }
        }
        if let Some(cmdline) = function_config.cmdline.as_ref() {
            args.extend_from_slice(&["--kernel_args", cmdline]);
        }

        // network config should be of the format <TAP-Name>/<MAC Address>
        //let tap_name = format!("tap{}", cid - 100);
        //let mac_addr = format!(
        //    "{}:{:02X}:{:02X}",
        //    MACPREFIX,
        //    ((cid - 100) & 0xff00) >> 8,
        //    (cid - 100) & 0xff
        //);
        if function_config.mac.is_some() {
            args.extend_from_slice(&["--tap_name", function_config.tap.as_ref().unwrap()]);
            args.extend_from_slice(&["--mac", function_config.mac.as_ref().unwrap()]);
        }

        // odirect
        if let Some(odirect) = odirect {
            if odirect.base {
                args.push("--odirect_base");
            }
            if !odirect.diff {
                args.push("--no_odirect_diff");
            }
            if !odirect.rootfs {
                args.push("--no_odirect_root");
            }
            if !odirect.appfs {
                args.push("--no_odirect_app");
            }
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap();
        let (conn, vm_process) = runtime.block_on(async {
            debug!("args: {:?}", args);
            let mut vm_process = Command::new("firerunner")
                .args(args)
                .kill_on_drop(true)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| Error::ProcessSpawn(e))?;

            if force_exit {
                let output = vm_process
                    .wait_with_output()
                    .await
                    .expect("failed to wait on child");
                let mut status = 0;
                if !output.status.success() {
                    eprintln!("{:?}", String::from_utf8_lossy(&output.stderr));
                    status = 1;
                }
                crate::unlink_unix_sockets();
                std::process::exit(status);
            }

            let vm_listener = tokio::net::UnixListener::from_std(vm_listener).unwrap();
            let conn = tokio::select! {
                res = vm_listener.accept() => {
                    res.unwrap().0.into_std().unwrap()
                },
                _ = vm_process.wait() => {
                    crate::unlink_unix_sockets();
                    error!("[Worker] cannot connect to the VM");
                    std::process::exit(1);
                }
            };
            conn.set_nonblocking(false)
                .map_err(|e| Error::VsockListen(e))?;
            let x: Result<_, Error> = Ok((conn, vm_process));
            x
        })?;

        let handle = VmHandle { conn, vm_process };

        self.handle = Some(handle);

        Ok(())
    }
}

impl SyscallChannel for Vm {
    fn send(&mut self, bytes: Vec<u8>) -> Result<(), SyscallChannelError> {
        let mut conn = &self.handle.as_ref().unwrap().conn;
        conn.write_all(&(bytes.len() as u32).to_be_bytes())
            .map_err(|e| {
                error!("write_all size {:?}", e);
                SyscallChannelError::Write
            })?;
        conn.write_all(bytes.as_ref()).map_err(|e| {
            error!("write_all contents {:?}", e);
            SyscallChannelError::Write
        })
    }

    fn wait(&mut self) -> Result<Option<SC>, SyscallChannelError> {
        let mut lenbuf = [0; 4];
        let mut conn = &self.handle.as_ref().unwrap().conn;
        conn.read_exact(&mut lenbuf).map_err(|e| {
            error!("read_exact size {:?}", e);
            SyscallChannelError::Read
        })?;
        let size = u32::from_be_bytes(lenbuf);
        let mut buf = vec![0u8; size as usize];
        conn.read_exact(&mut buf).map_err(|e| {
            error!("read_exact contents {:?}", e);
            SyscallChannelError::Read
        })?;
        let ret = syscalls::Syscall::decode(buf.as_ref())
            .map_err(|e| {
                error!("decode syscall {:?}", e);
                SyscallChannelError::Decode
            })?
            .syscall;
        Ok(ret)
    }
}

impl Drop for Vm {
    /// shutdown this vm
    fn drop(&mut self) {
        if let Some(handle) = self.handle.as_ref() {
            if let Err(e) = handle.conn.shutdown(Shutdown::Both) {
                error!("Failed to shut down unix connection: {:?}", e);
            } else {
                debug!("shutdown vm connection {:?}", handle.conn);
            }
        } else {
            debug!("dropping vm. unlaunched.")
        }
    }
}
