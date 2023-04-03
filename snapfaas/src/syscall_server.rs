use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, Write};
///! secure runtime that holds the handles to the VM and the global file system
use std::net::TcpStream;

use labeled::buckle::{self, Buckle, Clause, Component};
use lmdb::{Transaction, WriteFlags};
use log::{debug, error, warn};

use crate::blobstore::{self, Blobstore};
use crate::fs::{self, Function, FS};
use crate::labeled_fs::DBENV;
use crate::sched::{
    self,
    message::{ReturnCode, TaskReturn},
};
use crate::syscalls::{self, syscall::Syscall as SC};

const GITHUB_REST_ENDPOINT: &str = "https://api.github.com";
const GITHUB_REST_API_VERSION_HEADER: &str = "application/json+vnd";
const GITHUB_AUTH_TOKEN: &str = "GITHUB_AUTH_TOKEN";
const USER_AGENT: &str = "snapfaas";

pub fn pbcomponent_to_component(component: &Option<syscalls::Component>) -> Component {
    match component {
        None => Component::DCFalse,
        Some(set) => Component::DCFormula(
            set.clauses
                .iter()
                .map(|c| {
                    Clause(
                        c.principals
                            .iter()
                            .map(|p| p.tokens.iter().cloned().collect())
                            .collect(),
                    )
                })
                .collect(),
        ),
    }
}

pub fn pblabel_to_buckle(label: &syscalls::Buckle) -> Buckle {
    Buckle {
        secrecy: pbcomponent_to_component(&label.secrecy),
        integrity: pbcomponent_to_component(&label.integrity),
    }
}

pub fn component_to_pbcomponent(component: &Component) -> Option<syscalls::Component> {
    match component {
        Component::DCFalse => None,
        Component::DCFormula(set) => Some(syscalls::Component {
            clauses: set
                .iter()
                .map(|clause| syscalls::Clause {
                    principals: clause
                        .0
                        .iter()
                        .map(|vp| syscalls::TokenList { tokens: vp.clone() })
                        .collect(),
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
pub enum SyscallChannelError {
    Read,
    Decode,
    Write,
}

pub trait SyscallChannel {
    fn send(&mut self, bytes: Vec<u8>) -> Result<(), SyscallChannelError>;
    fn wait(&mut self) -> Result<Option<SC>, SyscallChannelError>;
}

#[derive(Debug)]
pub enum SyscallProcessorError {
    UnreachableScheduler,
    Channel(SyscallChannelError),
    Blob(std::io::Error),
    Database,
    Http(reqwest::Error),
    HttpAuth,
    BadStrPath,
    BadUrlArgs,
}

impl From<SyscallChannelError> for SyscallProcessorError {
    fn from(sce: SyscallChannelError) -> Self {
        SyscallProcessorError::Channel(sce)
    }
}

#[derive(Debug)]
pub struct SyscallGlobalEnv {
    pub sched_conn: Option<TcpStream>,
    pub db: lmdb::Database,
    pub fs: FS<&'static lmdb::Environment>,
    pub blobstore: Blobstore,
}

pub struct SyscallProcessor {
    create_blobs: HashMap<u64, blobstore::NewBlob>,
    blobs: HashMap<u64, blobstore::Blob>,
    max_blob_id: u64,
    http_client: reqwest::blocking::Client,
}

impl SyscallProcessor {
    pub fn new(label: Buckle, privilege: Component, clearance: Buckle) -> Self {
        {
            // set up label & privilege
            fs::utils::clear_label();
            fs::utils::taint_with_label(label);
            fs::utils::set_my_privilge(privilege);
            fs::utils::set_clearance(clearance);
        }

        Self {
            create_blobs: Default::default(),
            blobs: Default::default(),
            max_blob_id: 0,
            http_client: reqwest::blocking::Client::new(),
        }
    }

    pub fn new_insecure() -> Self {
        Self {
            create_blobs: Default::default(),
            blobs: Default::default(),
            max_blob_id: 0,
            http_client: reqwest::blocking::Client::new(),
        }
    }

    /// Send a HTTP GET request no matter if an authentication token is present
    fn http_get(
        &self,
        sc_req: &syscalls::GithubRest,
    ) -> Result<reqwest::blocking::Response, SyscallProcessorError> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        let mut req = self
            .http_client
            .get(url)
            .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
            .header(reqwest::header::USER_AGENT, USER_AGENT);
        req = match std::env::var_os(GITHUB_AUTH_TOKEN) {
            Some(t_osstr) => match t_osstr.into_string() {
                Ok(t_str) => req.bearer_auth(t_str),
                Err(_) => req,
            },
            None => req,
        };
        req.send().map_err(|e| SyscallProcessorError::Http(e))
    }

    /// Send a HTTP POST request only if an authentication token is present
    fn http_post(
        &self,
        sc_req: &syscalls::GithubRest,
        method: reqwest::Method,
    ) -> Result<reqwest::blocking::Response, SyscallProcessorError> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        match std::env::var_os(GITHUB_AUTH_TOKEN) {
            Some(t_osstr) => match t_osstr.into_string() {
                Ok(t_str) => self
                    .http_client
                    .request(method, url)
                    .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
                    .header(reqwest::header::USER_AGENT, USER_AGENT)
                    .body(std::string::String::from(sc_req.body.as_ref().unwrap()))
                    .bearer_auth(t_str)
                    .send()
                    .map_err(|e| SyscallProcessorError::Http(e)),
                Err(_) => Err(SyscallProcessorError::HttpAuth),
            },
            None => Err(SyscallProcessorError::HttpAuth),
        }
    }

    fn http_send(
        &self,
        service_info: &fs::ServiceInfo,
        body: Option<String>
    ) -> Result<reqwest::blocking::Response, SyscallProcessorError> {
        let url = service_info.url.clone();
        let method = service_info.verb.clone().into();
        let headers = service_info.headers
            .iter()
            .map(|(a, b)| (
                reqwest::header::HeaderName::from_bytes(a.as_bytes()).unwrap(),
                reqwest::header::HeaderValue::from_bytes(b.as_bytes()).unwrap()
            ))
            .collect::<reqwest::header::HeaderMap>();
        let mut request = self.http_client.request(method, url).headers(headers);
        if let Some(body) = body {
            request = request.body(body);
        }
        request.send().map_err(|e| SyscallProcessorError::Http(e))
    }

    pub fn run(
        mut self,
        env: &mut SyscallGlobalEnv,
        payload: String,
        s: &mut impl SyscallChannel,
    ) -> Result<TaskReturn, SyscallProcessorError> {
        use prost::Message;
        s.send(syscalls::Request { payload }.encode_to_vec())?;

        loop {
            let sc = s.wait()?;
            match sc {
                Some(SC::Response(r)) => {
                    debug!("function response: {}", r.payload);
                    return Ok(TaskReturn {
                        code: ReturnCode::Success as i32,
                        payload: Some(r.payload),
                    });
                }
                Some(SC::InvokeGate(i)) => {
                    let result = match env.sched_conn.as_mut() {
                        None => {
                            warn!("No scheduler presents. Syscall invoke is noop.");
                            syscalls::WriteKeyResponse { success: false }
                        }
                        Some(sched_conn) => {
                            // FIXME change to string path
                            let ret = fs::utils::invoke(&env.fs, i.gate).ok();
                            let result = syscalls::WriteKeyResponse {
                                success: ret.is_some(),
                            };
                            if ret.is_some() {
                                let (f, p) = ret.unwrap();
                                let label = fs::utils::get_current_label();
                                let sched_invoke = sched::message::LabeledInvoke {
                                    function: Some(f.into()),
                                    payload: i.payload,
                                    gate_privilege: component_to_pbcomponent(&p),
                                    label: Some(buckle_to_pblabel(&label)),
                                    sync: false,
                                };
                                sched::rpc::labeled_invoke(sched_conn, sched_invoke).map_err(
                                    |e| {
                                        error!("{:?}", e);
                                        SyscallProcessorError::UnreachableScheduler
                                    },
                                )?;
                            }
                            result
                        }
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::InvokeFunction(i)) => {
                    let result = match env.sched_conn.as_mut() {
                        None => {
                            warn!("No scheduler presents. Syscall invoke is noop.");
                            syscalls::WriteKeyResponse { success: false }
                        }
                        Some(sched_conn) => {
                            // read key i.function
                            let txn = DBENV.begin_ro_txn().unwrap();
                            let val = txn.get(env.db, &i.function).ok();
                            let result = syscalls::WriteKeyResponse {
                                success: val.is_some(),
                            };
                            if let Some(val) = val {
                                let f: Function = serde_json::from_slice(val).map_err(|e| {
                                    error!("{}", e.to_string());
                                    SyscallProcessorError::BadStrPath
                                })?;
                                let sched_invoke = sched::message::UnlabeledInvoke {
                                    function: Some(f.into()),
                                    payload: i.payload,
                                };
                                sched::rpc::unlabeled_invoke(sched_conn, sched_invoke).map_err(
                                    |e| {
                                        error!("{:?}", e);
                                        SyscallProcessorError::UnreachableScheduler
                                    },
                                )?;
                            }
                            result
                        }
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::InvokeService(req)) => {
                    let service = fs::utils::invoke_service(&env.fs, req.serv).ok();
                    let resp = match service {
                        Some(s) => {
                            fs::utils::taint_with_label(s.label.clone());
                            Some(self.http_send(&s, req.body)?)
                        }
                        None => None
                    };
                    let result = match resp {
                        None => {
                            syscalls::ServiceResponse {
                                data: "Fail to invoke the external service".as_bytes().to_vec(),
                                status: 0,
                            }
                        }
                        Some(resp) => {
                            syscalls::ServiceResponse {
                                status: resp.status().as_u16() as u32,
                                data: resp.bytes().map_err(|e| SyscallProcessorError::Http(e))?.to_vec(),
                            }
                        }
                    };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FsDelete(del)) => {
                    let value = fs::path::Path::parse(&del.path).ok().and_then(|p| {
                        p.file_name().and_then(|name| {
                            p.parent().and_then(|base_dir| {
                                fs::utils::delete(&env.fs, base_dir, name).ok()
                            })
                        })
                    });
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::BuckleParse(bstr)) => {
                    let result = syscalls::DeclassifyResponse {
                        label: Buckle::parse(&bstr).ok().map(|l| buckle_to_pblabel(&l)),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::SubPrivilege(suffix)) => {
                    // omnipotent privilege: dc_false + suffix = dc_false
                    // empty privilege: dc_true + suffix = dc_true
                    let mut my_priv = component_to_pbcomponent(&fs::utils::my_privilege());
                    if let Some(clauses) = my_priv.as_mut() {
                        if let Some(clause) = clauses.clauses.first_mut() {
                            clause
                                .principals
                                .first_mut()
                                .unwrap()
                                .tokens
                                .extend(suffix.tokens);
                        }
                    }
                    let result = syscalls::Buckle {
                        secrecy: my_priv,
                        integrity: None,
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::DupGate(req)) => {
                    let policy = pblabel_to_buckle(&req.policy.unwrap());
                    let value =
                        fs::utils::dup_gate(&env.fs, req.orig, req.base_dir, req.name, policy).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateGate(cg)) => {
                    let mut success = false;
                    if let Ok(path) = fs::path::Path::parse(&cg.path) {
                        if let Some(base_dir) = path.parent() {
                            if let Some(name) = path.file_name() {
                                if let Ok(policy) = buckle::Buckle::parse(&cg.policy) {
                                    let runtime_blob =
                                        fs::bootstrap::get_runtime_blob(&env.fs, &cg.runtime);
                                    let kernel_blob = fs::bootstrap::get_kernel_blob(&env.fs);
                                    let value = fs::utils::create_gate(
                                        &env.fs,
                                        base_dir,
                                        name,
                                        policy,
                                        fs::Function {
                                            memory: cg.memory as usize,
                                            app_image: cg.app_image,
                                            runtime_image: runtime_blob,
                                            kernel: kernel_blob,
                                        },
                                    );
                                    if value.is_err() {
                                        debug!("fs-create-gate failed: {:?}", value.unwrap_err());
                                    } else {
                                        success = true;
                                    }
                                }
                            }
                        }
                    }
                    let result = syscalls::WriteKeyResponse { success };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateRedirectGate(crg)) => {
                    let mut success = false;
                    if let Ok(path) = fs::path::Path::parse(&crg.path) {
                        if let Some(base_dir) = path.parent() {
                            if let Some(name) = path.file_name() {
                                if let Ok(policy) = buckle::Buckle::parse(&crg.policy) {
                                    if let Ok(redirect_path) =
                                        fs::path::Path::parse(&crg.redirect_path)
                                    {
                                        let value = fs::utils::create_redirect_gate(
                                            &env.fs,
                                            base_dir,
                                            name,
                                            policy,
                                            redirect_path,
                                        );
                                        if value.is_err() {
                                            debug!(
                                                "fs-create-redirect-gate failed: {:?}",
                                                value.unwrap_err()
                                            );
                                        } else {
                                            success = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let result = syscalls::WriteKeyResponse { success };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateService(cs)) => {
                    let mut success = false;
                    if let Ok(path) = fs::path::Path::parse(&cs.path) {
                        if let Some(base_dir) = path.parent() {
                            if let Some(name) = path.file_name() {
                                if let Ok(policy) = buckle::Buckle::parse(&cs.policy) {
                                    if let Ok(label) = buckle::Buckle::parse(&cs.label) {
                                        if let Ok(url) = reqwest::Url::parse(&cs.url).map(|u| u.to_string()) {
                                            if let Ok(verb) = reqwest::Method::from_bytes(cs.verb.as_bytes()) {
                                                const DEFAULT_HEADERS: &[(&str, &str)] = &[
                                                    ("ACCEPT", "*/*"),
                                                    ("USER_AGENT", "faasten"),
                                                ];
                                                let headers = cs.headers
                                                    .map_or_else(
                                                    || {
                                                        Ok(DEFAULT_HEADERS
                                                            .iter()
                                                            .map(|(n, v)| (n.to_string(), v.to_string()))
                                                            .collect::<HashMap<_, _>>())
                                                    },
                                                    |hs| {
                                                        serde_json::from_str(&hs)
                                                    });
                                                if let Ok(headers) = headers {
                                                    let value = fs::utils::create_service(
                                                        &env.fs,
                                                        base_dir,
                                                        name,
                                                        policy,
                                                        label,
                                                        url,
                                                        verb,
                                                        headers,
                                                    );
                                                    if value.is_err() {
                                                        debug!("fs-create-service failed: {:?}", value.unwrap_err());
                                                    } else {
                                                        success = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let result = syscalls::WriteKeyResponse { success };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::ReadKey(rk)) => {
                    let txn = DBENV.begin_ro_txn().unwrap();
                    let result = syscalls::ReadKeyResponse {
                        value: txn.get(env.db, &rk.key).ok().map(Vec::from),
                    };
                    let _ = txn.commit();
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::WriteKey(wk)) => {
                    let mut txn = DBENV.begin_rw_txn().unwrap();
                    let result = syscalls::WriteKeyResponse {
                        success: txn
                            .put(env.db, &wk.key, &wk.value, WriteFlags::empty())
                            .is_ok(),
                    };
                    let _ = txn.commit();
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::ReadDir(req)) => {
                    use lmdb::Cursor;
                    let mut keys: HashSet<Vec<u8>> = HashSet::new();

                    let txn = DBENV.begin_ro_txn().unwrap();
                    {
                        let mut dir = req.dir;
                        if !dir.ends_with(b"/") {
                            dir.push(b'/');
                        }
                        let mut cursor = txn
                            .open_ro_cursor(env.db)
                            .or(Err(SyscallProcessorError::Database))?
                            .iter_from(&dir);
                        while let Some(Ok((key, _))) = cursor.next() {
                            if !key.starts_with(&dir) {
                                break;
                            }
                            if let Some(entry) = key
                                .split_at(dir.len())
                                .1
                                .split_inclusive(|c| *c == b'/')
                                .next()
                            {
                                if !entry.is_empty() {
                                    keys.insert(entry.into());
                                }
                            }
                        }
                    }
                    let _ = txn.commit();

                    let result = syscalls::ReadDirResponse {
                        keys: keys.drain().collect(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsRead(rd)) => {
                    let value = fs::path::Path::parse(&rd.path)
                        .ok()
                        .and_then(|p| fs::utils::read(&env.fs, p).ok());
                    let result = syscalls::ReadKeyResponse { value };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsList(req)) => {
                    let value = fs::path::Path::parse(&req.path).ok().and_then(|p| {
                        fs::utils::list(&env.fs, p)
                            .ok()
                            .map(|m| syscalls::EntryNameArr {
                                names: m.keys().cloned().collect(),
                            })
                    });
                    let result = syscalls::FsListResponse { value };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsFacetedList(req)) => {
                    let value = fs::path::Path::parse(&req.path).ok().and_then(|p| {
                        fs::utils::faceted_list(&env.fs, p).ok().map(|facets| {
                            syscalls::FsFacetedListInner {
                                facets: facets
                                    .iter()
                                    .map(|(k, m)| {
                                        (
                                            k.clone(),
                                            syscalls::EntryNameArr {
                                                names: m.keys().cloned().collect(),
                                            },
                                        )
                                    })
                                    .collect::<HashMap<String, syscalls::EntryNameArr>>(),
                            }
                        })
                    });
                    let result = syscalls::FsFacetedListResponse { value };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsWrite(wr)) => {
                    let value = fs::path::Path::parse(&wr.path)
                        .ok()
                        .and_then(|p| fs::utils::write(&env.fs, p, wr.data).ok());
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateFacetedDir(req)) => {
                    let value = fs::path::Path::parse(&req.path).ok().and_then(|p| {
                        p.file_name().and_then(|name| {
                            p.parent().and_then(|base_dir| {
                                fs::utils::create_faceted(&env.fs, base_dir, name).ok()
                            })
                        })
                    });
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateDir(req)) => {
                    let value = fs::path::Path::parse(&req.path).ok().and_then(|p| {
                        p.file_name().and_then(|name| {
                            p.parent().and_then(|base_dir| {
                                buckle::Buckle::parse(&req.label).ok().and_then(|l| {
                                    fs::utils::create_directory(&env.fs, base_dir, name, l).ok()
                                })
                            })
                        })
                    });
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateFile(req)) => {
                    let value = fs::path::Path::parse(&req.path).ok().and_then(|p| {
                        p.file_name().and_then(|name| {
                            p.parent().and_then(|base_dir| {
                                buckle::Buckle::parse(&req.label).ok().and_then(|l| {
                                    fs::utils::create_file(&env.fs, base_dir, name, l).ok()
                                })
                            })
                        })
                    });
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateBlobByName(req)) => {
                    let mut success = false;
                    if let Ok(path) = fs::path::Path::parse(&req.path) {
                        if let Some(base_dir) = path.parent() {
                            if let Some(name) = path.file_name() {
                                let label = req.label.map_or_else(
                                    || fs::utils::get_current_label(),
                                    |lpb| pblabel_to_buckle(&lpb),
                                );
                                if fs::utils::create_blob(
                                    &env.fs,
                                    base_dir,
                                    name,
                                    label,
                                    req.blobname,
                                )
                                .is_ok()
                                {
                                    success = true;
                                }
                            }
                        }
                    }
                    let result = syscalls::WriteKeyResponse { success };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::GithubRest(req)) => {
                    let resp = match syscalls::HttpVerb::from_i32(req.verb) {
                        Some(syscalls::HttpVerb::Get) => Some(self.http_get(&req)?),
                        Some(syscalls::HttpVerb::Post) => {
                            Some(self.http_post(&req, reqwest::Method::POST)?)
                        }
                        Some(syscalls::HttpVerb::Put) => {
                            Some(self.http_post(&req, reqwest::Method::PUT)?)
                        }
                        Some(syscalls::HttpVerb::Delete) => {
                            Some(self.http_post(&req, reqwest::Method::DELETE)?)
                        }
                        None => None,
                    };
                    let result = match resp {
                        None => syscalls::GithubRestResponse {
                            data: format!("`{:?}` not supported", req.verb)
                                .as_bytes()
                                .to_vec(),
                            status: 0,
                        },
                        Some(mut resp) => {
                            if req.toblob && resp.status().is_success() {
                                let mut file = env
                                    .blobstore
                                    .create()
                                    .map_err(|e| SyscallProcessorError::Blob(e))?;
                                let mut buf = [0; 4096];
                                while let Ok(len) = resp.read(&mut buf) {
                                    if len == 0 {
                                        break;
                                    }
                                    let _ = file.write_all(&buf[0..len]);
                                }
                                let result = env
                                    .blobstore
                                    .save(file)
                                    .map_err(|e| SyscallProcessorError::Blob(e))?;
                                syscalls::GithubRestResponse {
                                    status: resp.status().as_u16() as u32,
                                    data: Vec::from(result.name),
                                }
                            } else {
                                syscalls::GithubRestResponse {
                                    status: resp.status().as_u16() as u32,
                                    data: resp
                                        .bytes()
                                        .map_err(|e| SyscallProcessorError::Http(e))?
                                        .to_vec(),
                                }
                            }
                        }
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::GetCurrentLabel(_)) => {
                    let result = buckle_to_pblabel(&fs::utils::get_current_label());
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::TaintWithLabel(label)) => {
                    let label = pblabel_to_buckle(&label);
                    let result = buckle_to_pblabel(&fs::utils::taint_with_label(label));
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::Declassify(target)) => {
                    let target = pbcomponent_to_component(&Some(target));
                    let result = syscalls::DeclassifyResponse {
                        label: fs::utils::declassify(target)
                            .map(|l| buckle_to_pblabel(&l))
                            .ok(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::CreateBlob(_cb)) => {
                    if let Ok(newblob) = env
                        .blobstore
                        .create()
                        .map_err(|e| SyscallProcessorError::Blob(e))
                    {
                        self.max_blob_id += 1;
                        self.create_blobs.insert(self.max_blob_id, newblob);

                        let result = syscalls::BlobResponse {
                            success: true,
                            fd: self.max_blob_id,
                            data: Vec::new(),
                        };
                        s.send(result.encode_to_vec())?;
                    } else {
                        let result = syscalls::BlobResponse {
                            success: false,
                            fd: 0,
                            data: Vec::new(),
                        };
                        s.send(result.encode_to_vec())?;
                    }
                }
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
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FinalizeBlob(fb)) => {
                    let result = if let Some(mut newblob) = self.create_blobs.remove(&fb.fd) {
                        let blob = newblob
                            .write_all(&fb.data)
                            .and_then(|_| env.blobstore.save(newblob))
                            .map_err(|e| SyscallProcessorError::Blob(e))?;
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
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::OpenBlob(ob)) => {
                    let result = if let Ok(file) = env.blobstore.open(ob.name) {
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
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::ReadBlob(rb)) => {
                    let result = if let Some(file) = self.blobs.get_mut(&rb.fd) {
                        let mut buf = Vec::from([0; 4096]);
                        let limit = std::cmp::min(rb.length.unwrap_or(4096), 4096) as usize;
                        if let Some(offset) = rb.offset {
                            file.seek(std::io::SeekFrom::Start(offset))
                                .map_err(|e| SyscallProcessorError::Blob(e))?;
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
                    s.send(result.encode_to_vec())?;
                }
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
                    s.send(result.encode_to_vec())?;
                }
                None => {
                    // Should never happen, so just ignore??
                }
            }
        }
    }
}
