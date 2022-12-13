//! Host-side VM handle that transfer data in and out of the VM through VSOCK socket and
//! implements syscall API
use std::env;
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Stdio;
use std::string::String;
use std::sync::mpsc::Sender;
use std::sync::mpsc;
use std::io::{Seek, Write};
use std::collections::{HashMap, HashSet};
use std::cell::RefCell;
use std::time::{Duration, Instant};

use labeled::buckle::{Clause, Component, Buckle};
use log::{debug, error};
use tokio::process::{Child, Command};
use::serde::Serialize;

use crate::configs::FunctionConfig;
use crate::message::Message;
use crate::request::LabeledInvoke;
use crate::{blobstore, syscalls};
use crate::labeled_fs::DBENV;
use crate::fs;

const MACPREFIX: &str = "AA:BB:CC:DD";
const GITHUB_REST_ENDPOINT: &str = "https://api.github.com";
const GITHUB_REST_API_VERSION_HEADER: &str = "application/json+vnd";
const GITHUB_AUTH_TOKEN: &str = "GITHUB_AUTH_TOKEN";
const USER_AGENT: &str = "snapfaas";

thread_local!(static STAT: RefCell<Metrics> = RefCell::new(Metrics::default()));

#[derive(Default, Serialize, Clone)]
struct Metrics {
    write: [Duration;4],
    list: [Duration;4],
    faceted_list: [Duration;4],
    read: [Duration;4],
    create: [Duration;4],
    delete: [Duration;4],
    buckle_parse: [Duration;4],
    sub_privilege: [Duration;4],
    declassify: [Duration;4],
    taint_with_label: [Duration;4],
    get_current_label: [Duration;4],
    endorse: [Duration;4],
}

fn get_stat() -> Metrics {
    STAT.with(|stat| {
        stat.borrow().clone()
    })
}

fn pbcomponent_to_component(component: &Option<syscalls::Component>) -> Component {
    match component {
        None => Component::DCFalse,
        Some(set) => Component::DCFormula(set.clauses.iter()
            .map(|c| Clause(c.principals.iter().map(|p| p.tokens.iter().cloned().collect()).collect()))
            .collect()),
    }
}

pub fn pblabel_to_buckle(label: &syscalls::Buckle) -> Buckle {
    Buckle {
        secrecy: pbcomponent_to_component(&label.secrecy),
        integrity: pbcomponent_to_component(&label.integrity),
    }
}

fn component_to_pbcomponent(component: &Component) -> Option<syscalls::Component> {
    match component {
        Component::DCFalse => None,
        Component::DCFormula(set) => Some(syscalls::Component {
            clauses: set
                .iter()
                .map(|clause| syscalls::Clause {
                    principals: clause.0.iter().map(|vp| syscalls::TokenList { tokens: vp.clone() }).collect(),
                })
                .collect(),
        }),
    }
}

pub fn buckle_to_pblabel(label: &Buckle) -> syscalls::Buckle {
    syscalls::Buckle {
        secrecy: component_to_pbcomponent(&label.secrecy),
        integrity: component_to_pbcomponent(&label.integrity),
    }
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
    pub label: Buckle,
    //privilege: Component,
    handle: Option<VmHandle>,
    blobstore: blobstore::Blobstore,
    create_blobs: HashMap<u64, blobstore::NewBlob>,
    blobs: HashMap<u64, blobstore::Blob>,
    max_blob_id: u64,
    fs: fs::FS<&'static lmdb::Environment>,
    start: Instant,
}

impl Vm {
    /// Create a new Vm instance with its handle uninitialized
    pub fn new(
        id: usize,
        firerunner: String,
        function_name: String,
        function_config: FunctionConfig,
        allow_network: bool,
        fs: fs::FS<&'static lmdb::Environment>,
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
            label: Buckle::new(true, true),
            //privilege: Component::formula([[function_name]]),
            handle: None,
            blobstore: Default::default(),
            create_blobs: Default::default(),
            blobs: Default::default(),
            max_blob_id: 0,
            fs,
            start: Instant::now(),
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
                    error!("[Worker] cannot connect to the VM");
                    std::process::exit(1);
                }
            };
            conn.set_nonblocking(false).map_err(|e| Error::VsockListen(e))?;
            let x: Result<_, Error> = Ok((conn, vm_process));
            x
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
    pub fn process_req(&mut self, payload: String) -> Result<String, Error> {
        use prost::Message;
        self.start = Instant::now();
        let sys_req = syscalls::Request {
            payload,
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

    fn sched_invoke(&self, invoke: LabeledInvoke) -> bool {
        use time::precise_time_ns;
        if let Some(invoke_handle) = self.handle.as_ref().and_then(|h| h.invoke_handle.as_ref()) {
            let (tx, _) = mpsc::channel();
            use crate::metrics::RequestTimestamps;
            let timestamps = RequestTimestamps {
                at_vmm: precise_time_ns(),
                //request: req.clone(),
                ..Default::default()
            };
            invoke_handle.send(Message::Request((invoke, tx, timestamps))).is_ok()
        } else {
            debug!("No invoke handle, ignoring invoke syscall.");
            false
        }
    }

    fn process_syscalls(&mut self) -> Result<String, Error> {
        use lmdb::{Transaction, WriteFlags};
        use prost::Message;
        use std::io::Read;
        use syscalls::syscall::Syscall as SC;
        use syscalls::Syscall;


        let default_db = DBENV.open_db(None);
        if default_db.is_err() {
            return Err(Error::DB(default_db.unwrap_err()));
        }

        let default_db = default_db.unwrap();
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
            let old = &fs::metrics::get_stat();
            match Syscall::decode(buf.as_ref()).map_err(|e| Error::Rpc(e))?.syscall {
                Some(SC::Response(r)) => {
                    debug!("function response: {}", r.payload);
                    let s = serde_json::json!({
                        "elapsed": self.start.elapsed(),
                        "fs": fs::metrics::get_stat(),
                        "syscall": get_stat(),
                        "response": serde_json::from_str::<serde_json::Value>(&r.payload).unwrap(),
                    }).to_string();
                    return Ok(s);
                }
                Some(SC::FsDelete(req)) => {
                    let now = Instant::now();
                    let result = syscalls::WriteKeyResponse {
                        success: fs::utils::delete(&self.fs, &req.base_dir, req.name).is_ok(),
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().delete[0] += now.elapsed();
                        stat.borrow_mut().delete[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().delete[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().delete[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::BuckleParse(s)) => {
                    let now = Instant::now();
                    let result = syscalls::DeclassifyResponse {
                        label: Buckle::parse(&s).ok().map(|l| buckle_to_pblabel(&l)),
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().buckle_parse[0] += now.elapsed();
                        stat.borrow_mut().buckle_parse[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().buckle_parse[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().buckle_parse[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::SubPrivilege(suffix)) => {
                    // omnipotent privilege: dc_false + suffix = dc_false
                    // empty privilege: dc_true + suffix = dc_true
                    let now = Instant::now();
                    let mut my_priv = component_to_pbcomponent(&fs::utils::my_privilege());
                    if let Some(clauses) = my_priv.as_mut() {
                        if let Some(clause) = clauses.clauses.first_mut() {
                            clause.principals.first_mut().unwrap().tokens.extend(suffix.tokens);
                        }
                    }
                    let result = syscalls::Buckle {
                        secrecy: my_priv,
                        integrity: None,
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().sub_privilege[0] += now.elapsed();
                        stat.borrow_mut().sub_privilege[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().sub_privilege[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().sub_privilege[3] += fs::metrics::calc_label_latencies(old);
                    });

                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::Invoke(req)) => {
                    let value = fs::utils::read_path(&self.fs, &req.gate).ok().and_then(|entry| {
                        match entry {
                            fs::DirEntry::Gate(gate) => self.fs.invoke_gate(&gate).ok(),
                            _ => None,
                        }
                    });
                    let mut result = syscalls::WriteKeyResponse { success: value.is_some() };
                    if value.is_some() {
                        let labeled = LabeledInvoke {
                            gate: value.clone().unwrap(),
                            label: fs::utils::get_current_label(),
                            payload: req.payload,
                        };
                        result.success = self.sched_invoke(labeled)
                    }
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::DupGate(req)) => {
                    let value = fs::utils::read_path(&self.fs, &req.orig).ok().and_then(|entry| { match entry {
                            fs::DirEntry::Gate(gate) => {
                                let policy = pblabel_to_buckle(&req.policy.unwrap());
                                fs::utils::create_gate(&self.fs, &req.base_dir, req.name, policy, gate.image).ok()
                            },
                            _ => None,
                        }
                    });
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    }
                    .encode_to_vec();
                    self.send_into_vm(result)?;
                },
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
                Some(SC::ReadDir(req)) => {
                    use lmdb::Cursor;
                    let mut keys: HashSet<Vec<u8>> = HashSet::new();

                    let txn = DBENV.begin_ro_txn().unwrap();
                    {
                        let mut dir = req.dir;
                        if !dir.ends_with(b"/") {
                            dir.push(b'/');
                        }
                        let mut cursor = txn.open_ro_cursor(default_db).or(Err(Error::RootfsNotExist))?.iter_from(&dir);
                        while let Some(Ok((key, _))) = cursor.next() {
                            if !key.starts_with(&dir) {
                                break
                            }
                            if let Some(entry) = key.split_at(dir.len()).1.split_inclusive(|c| *c == b'/').next() {
                                if !entry.is_empty() {
                                    keys.insert(entry.into());
                                }
                            }
                        }
                    }
                    let _ = txn.commit();

                    let result = syscalls::ReadDirResponse {
                        keys: keys.drain().collect(),
                    }.encode_to_vec();
                    self.send_into_vm(result)?;
                },
                Some(SC::FsRead(req)) => {
                    let now = Instant::now();
                    let value = fs::utils::read(&self.fs, &req.path).ok();
                    let result = syscalls::ReadKeyResponse {
                        value
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().read[0] += now.elapsed();
                        stat.borrow_mut().read[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().read[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().read[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::FsList(req)) => {
                    let now = Instant::now();
                    let value = fs::utils::list(&self.fs, &req.path).ok()
                        .map(|m| syscalls::EntryNameArr { names: m.keys().cloned().collect() });
                    let result = syscalls::FsListResponse { value };
                    STAT.with(|stat| {
                        stat.borrow_mut().list[0] += now.elapsed();
                        stat.borrow_mut().list[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().list[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().list[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::FsFacetedList(req)) => {
                    let now = Instant::now();
                    let value = fs::utils::faceted_list(&self.fs, &req.path).ok()
                        .map(|facets| {
                            syscalls::FsFacetedListInner{
                                facets: facets.iter().map(|(k, m)|
                                            (k.clone(), syscalls::EntryNameArr{ names: m.keys().cloned().collect() })
                                        ).collect::<HashMap<String, syscalls::EntryNameArr>>()
                            }
                        });
                    let result = syscalls::FsFacetedListResponse {
                        value
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().faceted_list[0] += now.elapsed();
                        stat.borrow_mut().faceted_list[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().faceted_list[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().faceted_list[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::FsWrite(req)) => {
                    let now = Instant::now();
                    let value = fs::utils::write(&mut self.fs, &req.path, req.data).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().write[0] += now.elapsed();
                        stat.borrow_mut().write[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().write[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().write[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::FsCreateFacetedDir(req)) => {
                    let now = Instant::now();
                    let value = fs::utils::create_faceted(&self.fs, &req.base_dir, req.name).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().create[0] += now.elapsed();
                        stat.borrow_mut().create[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().create[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().create[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::FsCreateDir(req)) => {
                    let now = Instant::now();
                    let label = pblabel_to_buckle(&req.label.clone().expect("label"));
                    let value = fs::utils::create_directory(&self.fs, &req.base_dir, req.name, label).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().create[0] += now.elapsed();
                        stat.borrow_mut().create[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().create[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().create[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::FsCreateFile(req)) => {
                    let now = Instant::now();
                    let label = pblabel_to_buckle(&req.label.clone().expect("label"));
                    let value = fs::utils::create_file(&self.fs, &req.base_dir, req.name, label).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().create[0] += now.elapsed();
                        stat.borrow_mut().create[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().create[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().create[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
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
                        Some(mut resp) => {
                            if req.toblob && resp.status().is_success() {
                                let mut file = self.blobstore.create()?;
                                let mut buf = [0; 4096];
                                while let Ok(len) = resp.read(&mut buf) {
                                    if len == 0 {
                                        break;
                                    }
                                    let _ = file.write_all(&buf[0..len]);
                                }
                                let result = self.blobstore.save(file)?;
                                syscalls::GithubRestResponse {
                                    status: resp.status().as_u16() as u32,
                                    data: Vec::from(result.name),
                                }
                            } else {
                                syscalls::GithubRestResponse {
                                    status: resp.status().as_u16() as u32,
                                    data: resp.bytes().map_err(|e| Error::HttpReq(e))?.to_vec(),
                                }
                            }
                        },
                    }.encode_to_vec();

                    self.send_into_vm(result)?;
                },
                Some(SC::GetCurrentLabel(_)) => {
                    let now = Instant::now();
                    let result = buckle_to_pblabel(&fs::utils::get_current_label());
                    STAT.with(|stat| {
                        stat.borrow_mut().get_current_label[0] += now.elapsed();
                        stat.borrow_mut().get_current_label[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().get_current_label[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().get_current_label[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::TaintWithLabel(label)) => {
                    let now = Instant::now();
                    let label = pblabel_to_buckle(&label);
                    let result = buckle_to_pblabel(&fs::utils::taint_with_label(label));
                    STAT.with(|stat| {
                        stat.borrow_mut().taint_with_label[0] += now.elapsed();
                        stat.borrow_mut().taint_with_label[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().taint_with_label[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().taint_with_label[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::Endorse(p)) => {
                    let now = Instant::now();
                    let p = pbcomponent_to_component(&Some(p));
                    let result = syscalls::DeclassifyResponse{
                        label: fs::utils::endorse_checked(&p).map(|l| buckle_to_pblabel(&l)).ok()
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().endorse[0] += now.elapsed();
                        stat.borrow_mut().endorse[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().endorse[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().endorse[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::Declassify(target)) => {
                    let now = Instant::now();
                    let target = pbcomponent_to_component(&Some(target));
                    let result = syscalls::DeclassifyResponse{
                        label: fs::utils::declassify(target).map(|l| buckle_to_pblabel(&l)).ok(),
                    };
                    STAT.with(|stat| {
                        stat.borrow_mut().declassify[0] += now.elapsed();
                        stat.borrow_mut().declassify[1] += fs::metrics::calc_store_latencies(old);
                        stat.borrow_mut().declassify[2] += fs::metrics::calc_serde_latencies(old);
                        stat.borrow_mut().declassify[3] += fs::metrics::calc_label_latencies(old);
                    });
                    self.send_into_vm(result.encode_to_vec())?;
                }
                Some(SC::CreateBlob(_cb)) => {
                    if let Ok(newblob) = self.blobstore.create().map_err(|_e| Error::AppfsNotExist) {
                        self.max_blob_id += 1;
                        self.create_blobs.insert(self.max_blob_id, newblob);

                        let result = syscalls::BlobResponse {
                            success: true,
                            fd: self.max_blob_id,
                            data: Vec::new(),
                        };
                        self.send_into_vm(result.encode_to_vec())?;
                    } else {
                        let result = syscalls::BlobResponse {
                            success: false,
                            fd: 0,
                            data: Vec::new(),
                        };
                        self.send_into_vm(result.encode_to_vec())?;
                    }
                },
                Some(SC::WriteBlob(wb)) => {
                    let result = if let Some(newblob) = self.create_blobs.get_mut(&wb.fd) {
                        let data = wb.data.as_ref();
                        if newblob.write_all(data).is_ok() {
                            syscalls::BlobResponse {
                                success: true,
                                fd: wb.fd,
                                data: Vec::new(),
                            }
                        } else {
                            syscalls::BlobResponse {
                                success: false,
                                fd: wb.fd,
                                data: Vec::from("Failed to write"),
                            }
                        }
                    } else {
                        syscalls::BlobResponse {
                            success: false,
                            fd: wb.fd,
                            data: Vec::from("Blob doesn't exist"),
                        }
                    };
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::FinalizeBlob(fb)) => {
                    let result = if let Some(mut newblob) = self.create_blobs.remove(&fb.fd) {
                        let blob = newblob.write_all(&fb.data).and_then(|_| self.blobstore.save(newblob))?;
                        syscalls::BlobResponse {
                            success: true,
                            fd: fb.fd,
                            data: Vec::from(blob.name),
                        }
                    } else {
                        syscalls::BlobResponse {
                            success: false,
                            fd: fb.fd,
                            data: Vec::from("Blob doesn't exist"),
                        }
                    };
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::OpenBlob(ob)) => {
                    let result = if let Ok(file) = self.blobstore.open(ob.name) {
                        self.max_blob_id += 1;
                        self.blobs.insert(self.max_blob_id, file);
                        syscalls::BlobResponse {
                            success: true,
                            fd: self.max_blob_id,
                            data: Vec::new(),
                        }
                    } else {
                        syscalls::BlobResponse {
                            success: false,
                            fd: 0,
                            data: Vec::new(),
                        }
                    };
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::ReadBlob(rb)) => {
                    let result = if let Some(file) = self.blobs.get_mut(&rb.fd) {
                        let mut buf = Vec::from([0; 4096]);
                        let limit = std::cmp::min(rb.length.unwrap_or(4096), 4096) as usize;
                        if let Some(offset) = rb.offset {
                            file.seek(std::io::SeekFrom::Start(offset))?;
                        }
                        if let Ok(len) = file.read(&mut buf[0..limit]) {
                            buf.truncate(len);
                            syscalls::BlobResponse {
                                success: true,
                                fd: rb.fd,
                                data: buf,
                            }
                        } else {
                            syscalls::BlobResponse {
                                success: false,
                                fd: rb.fd,
                                data: Vec::new(),
                            }
                        }
                    } else {
                            syscalls::BlobResponse {
                                success: false,
                                fd: rb.fd,
                                data: Vec::new(),
                            }
                    };
                    self.send_into_vm(result.encode_to_vec())?;
                },
                Some(SC::CloseBlob(cb)) => {
                    let result = if self.blobs.remove(&cb.fd).is_some() {
                        syscalls::BlobResponse {
                            success: true,
                            fd: cb.fd,
                            data: Vec::new(),
                        }
                    } else {
                        syscalls::BlobResponse {
                            success: false,
                            fd: cb.fd,
                            data: Vec::new(),
                        }
                    };
                    self.send_into_vm(result.encode_to_vec())?;
                },
                None => {
                    // Should never happen, so just ignore??
                    error!("received an unknown syscall");
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
