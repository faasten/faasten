//! Host-side VM handle that transfer data in and out of the VM through VSOCK socket and
//! implements syscall API
extern crate reqwest;
extern crate url;

#[cfg(not(test))]
use std::env;
#[cfg(not(test))]
use std::net::Shutdown;
#[cfg(not(test))]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(not(test))]
use tokio::process::{Child, Command};
use std::process::Stdio;
use std::string::String;
#[cfg(not(test))]
use log::{info, error};
use std::time::Instant;

#[cfg(not(test))]
use crate::configs::FunctionConfig;
#[cfg(not(test))]
use crate::request::Request;
use crate::syscalls;

const MACPREFIX: &str = "AA:BB:CC:DD";
const GITHUB_REST_ENDPOINT: &str = "https://api.github.com";
const GITHUB_REST_API_VERSION_HEADER: &str = "application/json+vnd";
const GITHUB_AUTH_TOKEN: &str = "GITHUB_AUTH_TOKEN";
const USER_AGENT: &str = "snapfaas";

use labeled::dclabel::{Clause, Component, DCLabel};
use labeled::Label;

#[derive(Debug)]
pub enum Error {
    ProcessSpawn(std::io::Error),
    //VmTimeout(RecvTimeoutError),
    VmWrite(std::io::Error),
    VmRead(std::io::Error),
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
    NotString(std::string::FromUtf8Error),
}

pub struct OdirectOption {
    pub base: bool,
    pub diff: bool,
    pub rootfs: bool,
    pub appfs: bool,
}

#[derive(Debug)]
#[cfg(not(test))]
pub struct Vm {
    pub id: usize,
    pub memory: usize, // MB
    pub function_name: String,
    conn: UnixStream,
    //TODO: currently every VM instance opens a connection to the REST server
    // Having a pool of connections is more ideal.
    rest_client: reqwest::blocking::Client,
    process: Child,
    current_label: DCLabel,
}

#[cfg(not(test))]
impl Vm {
    /// Launch a vm instance and return a Vm value
    /// When this function returns, the VM has finished booting and is ready
    /// to accept requests.
    pub fn new(
        id: &str,
        function_name: &str,
        function_config: &FunctionConfig,
        vm_listener: UnixListener,
        cid: u32,
        allow_network: bool,
        firerunner: &str,
        force_exit: bool,
        odirect: Option<OdirectOption>,
    ) -> Result<(Vm, Vec<Instant>), Error> {
        let mut ts_vec = Vec::with_capacity(10);
        ts_vec.push(Instant::now());
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
        if function_config.network && allow_network {
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
            info!("args: {:?}", args);
            let mut vm_process = Command::new(firerunner).args(args).kill_on_drop(true)
                .stdin(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| Error::ProcessSpawn(e))?;
            ts_vec.push(Instant::now());


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
        ts_vec.push(Instant::now());
        info!("{}", "VM is now connected to the host");

        let rest_client = reqwest::blocking::Client::new();

        return Ok((
            Vm {
                id: id.parse::<usize>().expect("vm id not int"),
                function_name: function_name.to_string(),
                memory: function_config.memory,
                conn,
                rest_client,
                process: vm_process,
                // We should also probably have a clearance to mitigate side channel attacks, but
                // meh for now...
                /// Starting label with public secrecy and integrity has app-name
                current_label: DCLabel::new(false, [[function_name.to_string()]]),
            },
            ts_vec,
        ));
    }

    /// Send request to vm and wait for its response
    pub fn process_req(&mut self, req: Request) -> Result<String, Error> {
        use prost::Message;
        use std::io::Write;

        let sys_req = syscalls::Request {
            payload: req.payload_as_string(),
        }
        .encode_to_vec();

        self.conn.write_all(&(sys_req.len() as u32).to_be_bytes()).map_err(|e| Error::VsockWrite(e))?;
        self.conn.write_all(sys_req.as_ref()).map_err(|e| Error::VsockWrite(e))?;

        self.process_syscall()
    }

    /// Send a HTTP GET request no matter if an authentication token is present
    fn http_get(&self, sc_req: &syscalls::GithubRest) -> Result<reqwest::blocking::Response, Error> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        let mut req = self.rest_client.get(url)
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
    fn http_post(&self, sc_req: &syscalls::GithubRest) -> Result<reqwest::blocking::Response, Error> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        match env::var_os(GITHUB_AUTH_TOKEN) {
            Some(t_osstr) => {
                match t_osstr.into_string() {
                    Ok(t_str) => {
                        self.rest_client.post(url)
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

    pub fn process_syscall(&mut self) -> Result<String, Error> {
        use lmdb::{Transaction, WriteFlags};
        use prost::Message;
        use std::io::{Read, Write};
        use syscalls::syscall::Syscall as SC;
        use syscalls::Syscall;

        let dbenv = lmdb::Environment::new()
            .open(std::path::Path::new("storage"))
            .unwrap();
        let default_db = dbenv.open_db(None).unwrap();

        loop {
            let buf = {
                let mut lenbuf = [0;4];
                self.conn.read_exact(&mut lenbuf).map_err(|e| Error::VsockRead(e))?;
                let size = u32::from_be_bytes(lenbuf);
                let mut buf = vec![0u8; size as usize];
                self.conn.read_exact(&mut buf).map_err(|e| Error::VsockRead(e))?;
                buf
            };
            //println!("VM.RS: received a syscall");
            match Syscall::decode(buf.as_ref()).map_err(|e| Error::Rpc(e))?.syscall {
                Some(SC::Response(r)) => {
                    //println!("VS.RS: received response");
                    return Ok(r.payload);
                }
                Some(SC::ReadKey(rk)) => {
                    let txn = dbenv.begin_ro_txn().unwrap();
                    let result = syscalls::ReadKeyResponse {
                        value: txn.get(default_db, &rk.key).ok().map(Vec::from),
                    };
                    let _ = txn.commit();
                    self.conn.write_all(&(result.encoded_len() as u32).to_be_bytes()).map_err(|e| Error::VsockWrite(e))?;
                    self.conn.write_all(result.encode_to_vec().as_ref()).map_err(|e| Error::VsockWrite(e))?;
                },
                Some(SC::WriteKey(wk)) => {
                    let mut txn = dbenv.begin_rw_txn().unwrap();
                    let result = syscalls::WriteKeyResponse {
                        success: txn
                            .put(default_db, &wk.key, &wk.value, WriteFlags::empty())
                            .is_ok(),
                    };
                    let _ = txn.commit();
                    self.conn.write_all(&(result.encoded_len() as u32).to_be_bytes()).map_err(|e| Error::VsockWrite(e))?;
                    self.conn.write_all(result.encode_to_vec().as_ref()).map_err(|e| Error::VsockWrite(e))?;
                },
                Some(SC::GithubRest(req)) => {
                    let response = syscalls::GithubRestResponse{
                        data: match syscalls::HttpVerb::from_i32(req.verb) {
                            Some(syscalls::HttpVerb::Get) => {
                                self.http_get(&req)?.bytes().map_err(|e| Error::HttpReq(e))?.to_vec()
                            },
                            Some(syscalls::HttpVerb::Post) => {
                                self.http_post(&req)?.bytes().map_err(|e| Error::HttpReq(e))?.to_vec()
                            },
                            None => {
                               format!("`{:?}` not supported", req.verb).as_bytes().to_vec()
                            }
                        },
                    };
                    self.conn.write_all(&(response.encoded_len() as u32).to_be_bytes()).map_err(|e| Error::VsockWrite(e))?;
                    self.conn.write(response.encode_to_vec().as_ref()).map_err(|e| Error::VsockWrite(e))?;
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
                    };
                    self.conn
                        .write_all(&(result.encoded_len() as u32).to_be_bytes()).map_err(|e| Error::VsockWrite(e))?;
                    self.conn.write_all(result.encode_to_vec().as_ref()).map_err(|e| Error::VsockWrite(e))?;
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
                    };
                    self.conn
                        .write_all(&(result.encoded_len() as u32).to_be_bytes()).map_err(|e| Error::VsockWrite(e))?;
                    self.conn.write_all(result.encode_to_vec().as_ref()).map_err(|e| Error::VsockWrite(e))?;
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
        if let Err(e) = self.conn.shutdown(Shutdown::Both) {
            error!("Failed to shut down unix connection: {:?}", e);
        }
    }
}

#[cfg(test)]
use crate::request::Request;

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
        Vm { id: 0, memory }
    }

    pub fn process_req(&mut self, _: Request) -> Result<String, Error> {
        Ok(String::new())
    }

    pub fn shutdown(&mut self) {}
}
