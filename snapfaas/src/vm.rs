//! Host-side VM handle that transfer data in and out of the VM through VSOCK socket and
//! implements syscall API
use std::env;
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Stdio;
use std::string::String;
use std::sync::mpsc::Sender;
use std::sync::mpsc;
use std::io::Write;

use log::{debug, error};
use tokio::process::{Child, Command};
use serde_json::Value;

use crate::configs::FunctionConfig;
use crate::message::Message;
use crate::syscalls;
use crate::request::Request;

const MACPREFIX: &str = "AA:BB:CC:DD";
const GITHUB_REST_ENDPOINT: &str = "https://api.github.com";
const GITHUB_REST_API_VERSION_HEADER: &str = "application/json+vnd";
const GITHUB_AUTH_TOKEN: &str = "GITHUB_AUTH_TOKEN";
const USER_AGENT: &str = "snapfaas";

use labeled::dclabel::{Clause, Component, DCLabel};
use labeled::Label;

lazy_static::lazy_static! {
    static ref DBENV: lmdb::Environment = lmdb::Environment::new()
        .set_map_size(4096 * 1024 * 1024)
        .open(std::path::Path::new("storage"))
        .unwrap();
}

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
}

/// Specify the `O_DIRECT` flag when open a disk image which is a regular file
pub struct OdirectOption {
    pub base: bool,
    pub diff: bool,
    pub rootfs: bool,
    pub appfs: bool,
}

#[derive(Debug)]
struct VmHandle {
    conn: UnixStream,
    //currently every VM instance opens a connection to the REST server
    rest_client: reqwest::blocking::Client,
    #[allow(dead_code)]
    // This field is never used, but we need to it make sure the Child isn't dropped and, thus,
    // killed, before the VmHandle is dropped.
    vm_process: Child,
    // None when VM is created from single-VM launcher
    invoke_handle: Option<Sender<Message>>,
}

#[derive(Debug)]
pub struct Vm {
    id: usize,
    firerunner: String,
    allow_network: bool,
    function_name: String,
    function_config: FunctionConfig,
    current_label: DCLabel,
    handle: Option<VmHandle>,
}

impl Vm {
    /// Create a new Vm instance with its handle uninitialized
    pub fn new(
        id: usize,
        firerunner: String,
        function_name: String,
        function_config: FunctionConfig,
        allow_network: bool,
    ) -> Self {
        Vm {
            id,
            allow_network,
            firerunner,
            function_name: function_name.clone(),
            function_config,
            // We should also probably have a clearance to mitigate side channel attacks, but
            // meh for now...
            /// Starting label with public secrecy and integrity has app-name
            current_label: DCLabel::new(false, [[function_name]]),
            handle: None,
        }
    }

    /// Return true if the Vm instance is already launched, otherwise false.
    pub fn is_launched(&self) -> bool {
        self.handle.is_some()
    }

    /// Launch the current Vm instance.
    /// When this function returns, the VM has finished booting and is ready to accept requests.
    pub fn launch(
        &mut self,
        invoke_handle: Option<Sender<Message>>,
        vm_listener: UnixListener,
        cid: u32,
        force_exit: bool,
        odirect: Option<OdirectOption>,
    ) -> Result<(), Error> {
        let function_config = &self.function_config;
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
        }
        if let Some(dump_dir) = function_config.dump_dir.as_ref() {
            args.extend_from_slice(&["--dump_to", dump_dir]);
        }
        if let Some(cmdline) = function_config.cmdline.as_ref() {
            args.extend_from_slice(&["--kernel_args", cmdline]);
        }
        if function_config.dump_ws {
            args.push("--dump_ws");
        }
        if function_config.load_ws {
            args.push("--load_ws");
        }
        if function_config.copy_base {
            args.push("--copy_base");
        }
        if function_config.copy_diff {
            args.push("--copy_diff");
        }

        // network config should be of the format <TAP-Name>/<MAC Address>
        let tap_name = format!("tap{}", cid-100);
        let mac_addr = format!("{}:{:02X}:{:02X}", MACPREFIX, ((cid-100)&0xff00)>>8, (cid-100)&0xff);
        if function_config.network && self.allow_network {
            args.extend_from_slice(&["--tap_name", &tap_name]);
            args.extend_from_slice(&["--mac", &mac_addr]);
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

        let runtime = tokio::runtime::Builder::new_current_thread().enable_io().build().unwrap();
        let (conn, vm_process) = runtime.block_on(async {
            debug!("args: {:?}", args);
            let mut vm_process = Command::new(&self.firerunner).args(args).kill_on_drop(true)
                .stdin(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| Error::ProcessSpawn(e))?;

            if force_exit {
                let output = vm_process .wait_with_output().await
                    .expect("failed to wait on child");
                let mut status = 0;
                if !output.status.success() {
                    eprintln!("{:?}", String::from_utf8_lossy(&output.stderr));
                    status = 1;
                }
                crate::unlink_unix_sockets();
                std::process::exit(status);
            }

            let vm_listener = tokio::net::UnixListener::from_std(vm_listener).expect("convert from UnixListener std");
            let conn = tokio::select! {
                res = vm_listener.accept() => {
                    res.unwrap().0.into_std().unwrap()
                },
                _ = vm_process.wait() => {
                    crate::unlink_unix_sockets();
                    std::process::exit(1);
                }
            };
            conn.set_nonblocking(false).map_err(|e| Error::VsockListen(e))?;
            Ok((conn, vm_process))
        })?;

        let rest_client = reqwest::blocking::Client::new();
        
        let handle = VmHandle {
            conn,
            rest_client,
            vm_process,
            invoke_handle,
        };

        self.handle = Some(handle);

        Ok(())
    }

    pub fn function_name(&self) -> String {
        self.function_name.clone()
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Return function memory size in MB
    pub fn memory(&self) -> usize {
        self.function_config.memory
    }

    fn send_into_vm(&mut self, sys_req: Vec<u8>) -> Result<(), Error> {
        let mut conn = &self.handle.as_ref().unwrap().conn;
        conn.write_all(&(sys_req.len() as u32).to_be_bytes()).map_err(|e| Error::VsockWrite(e))?;
        conn.write_all(sys_req.as_ref()).map_err(|e| Error::VsockWrite(e))
    }

    /// Send request to vm and wait for its response
    pub fn process_req(&mut self, req: Value) -> Result<String, Error> {
        use prost::Message;

        let sys_req = syscalls::Request {
            payload: req.to_string(),
        }
        .encode_to_vec();

        self.send_into_vm(sys_req)?;

        self.process_syscalls()
    }

    /// Send a HTTP GET request no matter if an authentication token is present
    fn http_get(&self, sc_req: &syscalls::GithubRest) -> Result<reqwest::blocking::Response, Error> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        let rest_client = &self.handle.as_ref().unwrap().rest_client;
        let mut req = rest_client.get(url)
            .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
            .header(reqwest::header::USER_AGENT, USER_AGENT);
        req = match env::var_os(GITHUB_AUTH_TOKEN) {
            Some(t_osstr) => {
                match t_osstr.into_string() {
                    Ok(t_str) => req.bearer_auth(t_str),
                    Err(_) => req,
                }
            },
            None => req
        };
        req.send().map_err(|e| Error::HttpReq(e))
    }

    /// Send a HTTP POST request only if an authentication token is present
    fn http_post(&self, sc_req: &syscalls::GithubRest, method: reqwest::Method) -> Result<reqwest::blocking::Response, Error> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        match env::var_os(GITHUB_AUTH_TOKEN) {
            Some(t_osstr) => {
                match t_osstr.into_string() {
                    Ok(t_str) => {
                        let rest_client = &self.handle.as_ref().unwrap().rest_client;
                        rest_client.request(method, url)
                            .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
                            .header(reqwest::header::USER_AGENT, USER_AGENT)
                            .body(std::string::String::from(sc_req.body.as_ref().unwrap()))
                            .bearer_auth(t_str)
                            .send().map_err(|e| Error::HttpReq(e))
                    },
                    Err(_) => Err(Error::AuthTokenInvalid),
                }
            }
            None => Err(Error::AuthTokenNotExist),
        }
    }

    fn send_req(&self, invoke: syscalls::Invoke) -> bool {
        if let Some(invoke_handle) = self.handle.as_ref().and_then(|h| h.invoke_handle.as_ref()) {
            let (tx, _) = mpsc::channel();
            invoke_handle.send(Message::Request(
                Request {
                    function: invoke.function,
                    payload: serde_json::from_str(invoke.payload.as_str()).expect("json"),
                },
                tx,
            )).is_ok()
        } else {
            debug!("No invoke handle, ignoring invoke syscall. {:?}", invoke);
            false
        }
    }

    fn process_syscalls(&mut self) -> Result<String, Error> {
        use lmdb::{Transaction, WriteFlags};
        use prost::Message;
        use std::io::Read;
        use syscalls::syscall::Syscall as SC;
        use syscalls::Syscall;

        
        let default_db = DBENV.open_db(None).unwrap();

        loop {
            let buf = {
                let mut lenbuf = [0;4];
                let mut conn = &self.handle.as_ref().unwrap().conn;
                conn.read_exact(&mut lenbuf).map_err(|e| Error::VsockRead(e))?;
                let size = u32::from_be_bytes(lenbuf);
                let mut buf = vec![0u8; size as usize];
                conn.read_exact(&mut buf).map_err(|e| Error::VsockRead(e))?;
                buf
            };
            match Syscall::decode(buf.as_ref()).map_err(|e| Error::Rpc(e))?.syscall {
                Some(SC::Response(r)) => {
                    return Ok(r.payload);
                }
                Some(SC::Invoke(invoke)) => {
                    let result = syscalls::InvokeResponse { success: self.send_req(invoke) };
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::ReadKey(rk)) => {
                    let txn = DBENV.begin_ro_txn().unwrap();
                    let result = syscalls::ReadKeyResponse {
                        value: txn.get(default_db, &rk.key).ok().map(Vec::from),
                    }
                    .encode_to_vec();
                    let _ = txn.commit();

                    self.send_into_vm(result)?;
                },
                Some(SC::WriteKey(wk)) => {
                    let mut txn = DBENV.begin_rw_txn().unwrap();
                    let result = syscalls::WriteKeyResponse {
                        success: txn
                            .put(default_db, &wk.key, &wk.value, WriteFlags::empty())
                            .is_ok(),
                    }
                    .encode_to_vec();
                    let _ = txn.commit();

                    self.send_into_vm(result)?;
                },
                Some(SC::GithubRest(req)) => {
                    let resp = match syscalls::HttpVerb::from_i32(req.verb) {
                        Some(syscalls::HttpVerb::Get) => {
                            Some(self.http_get(&req)?)
                        },
                        Some(syscalls::HttpVerb::Post) => {
                            Some(self.http_post(&req, reqwest::Method::POST)?)
                        },
                        Some(syscalls::HttpVerb::Put) => {
                            Some(self.http_post(&req, reqwest::Method::PUT)?)
                        },
                        Some(syscalls::HttpVerb::Delete) => {
                            Some(self.http_post(&req, reqwest::Method::DELETE)?)
                        },
                        None => {
                           None
                        }
                    };
                    let result = match resp {
                        None => syscalls::GithubRestResponse {
                            data: format!("`{:?}` not supported", req.verb).as_bytes().to_vec(),
                            status: 0,
                        },
                        Some(resp) => syscalls::GithubRestResponse {
                            status: resp.status().as_u16() as u32,
                            data: resp.bytes().map_err(|e| Error::HttpReq(e))?.to_vec(),
                        }
                    }.encode_to_vec();

                    self.send_into_vm(result)?;
                },
                Some(SC::GetCurrentLabel(_)) => {
                    let result = syscalls::DcLabel {
                        secrecy: match &self.current_label.secrecy {
                            Component::DCFalse => None,
                            Component::DCFormula(set) => Some(syscalls::Component {
                                clauses: set
                                    .iter()
                                    .map(|clause| syscalls::Clause {
                                        principals: clause.0.iter().map(Clone::clone).collect(),
                                    })
                                    .collect(),
                            }),
                        },
                        integrity: match &self.current_label.integrity {
                            Component::DCFalse => None,
                            Component::DCFormula(set) => Some(syscalls::Component {
                                clauses: set
                                    .iter()
                                    .map(|clause| syscalls::Clause {
                                        principals: clause.0.iter().map(Clone::clone).collect(),
                                    })
                                    .collect(),
                            }),
                        },
                    }
                    .encode_to_vec();

                    self.send_into_vm(result)?;
                }
                Some(SC::TaintWithLabel(label)) => {
                    let dclabel = DCLabel {
                        secrecy: match label.secrecy {
                            None => Component::DCFalse,
                            Some(set) => Component::DCFormula(
                                set.clauses
                                    .iter()
                                    .map(|c| {
                                        Clause(c.principals.iter().map(Clone::clone).collect())
                                    })
                                    .collect(),
                            ),
                        },
                        integrity: match label.integrity {
                            None => Component::DCFalse,
                            Some(set) => Component::DCFormula(
                                set.clauses
                                    .iter()
                                    .map(|c| {
                                        Clause(c.principals.iter().map(Clone::clone).collect())
                                    })
                                    .collect(),
                            ),
                        },
                    };
                    self.current_label = self.current_label.clone().lub(dclabel);
                    let result = syscalls::DcLabel {
                        secrecy: match &self.current_label.secrecy {
                            Component::DCFalse => None,
                            Component::DCFormula(set) => Some(syscalls::Component {
                                clauses: set
                                    .iter()
                                    .map(|clause| syscalls::Clause {
                                        principals: clause.0.iter().map(Clone::clone).collect(),
                                    })
                                    .collect(),
                            }),
                        },
                        integrity: match &self.current_label.integrity {
                            Component::DCFalse => None,
                            Component::DCFormula(set) => Some(syscalls::Component {
                                clauses: set
                                    .iter()
                                    .map(|clause| syscalls::Clause {
                                        principals: clause.0.iter().map(Clone::clone).collect(),
                                    })
                                    .collect(),
                            }),
                        },
                    }
                    .encode_to_vec();

                    self.send_into_vm(result)?;
                }
                None => {
                    // Should never happen, so just ignore??
                    eprintln!("received an unknown syscall");
                },
            }
        }
    }
}

impl Drop for Vm {
    /// shutdown this vm
    fn drop(&mut self) {
        let handle = self.handle.as_ref().unwrap();
        if let Err(e) = handle.conn.shutdown(Shutdown::Both) {
            error!("Failed to shut down unix connection: {:?}", e);
        }
    }
}
