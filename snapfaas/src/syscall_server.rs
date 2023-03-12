///! secure runtime that holds the handles to the VM and the global file system
use std::net::TcpStream;
use std::collections::{HashMap, HashSet};
use std::io::{Seek, Read, Write};

use labeled::buckle::{Buckle, Component, Clause};
use log::{warn, error, debug};
use lmdb::{WriteFlags, Transaction};

use crate::syscalls::{self, syscall::Syscall as SC};
use crate::sched::{self, message::{TaskReturn, ReturnCode}};
use crate::fs::{self, FS};
use crate::blobstore::{self, Blobstore};
use crate::labeled_fs::DBENV;

const GITHUB_REST_ENDPOINT: &str = "https://api.github.com";
const GITHUB_REST_API_VERSION_HEADER: &str = "application/json+vnd";
const GITHUB_AUTH_TOKEN: &str = "GITHUB_AUTH_TOKEN";
const USER_AGENT: &str = "snapfaas";

pub fn pbcomponent_to_component(component: &Option<syscalls::Component>) -> Component {
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

pub fn component_to_pbcomponent(component: &Component) -> Option<syscalls::Component> {
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
    pub fn new(label: Buckle, privilege: Component) -> Self {
        {
            // setup label & privilege
            fs::utils::clear_label();
            fs::utils::taint_with_label(label);
            fs::utils::set_my_privilge(privilege);
        }

        Self {
            create_blobs: Default::default(),
            blobs: Default::default(),
            max_blob_id: 0,
            http_client: reqwest::blocking::Client::new(),
        }
    }

    /// Send a HTTP GET request no matter if an authentication token is present
    fn http_get(&self, sc_req: &syscalls::GithubRest) -> Result<reqwest::blocking::Response, SyscallProcessorError> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        let mut req = self.http_client.get(url)
            .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
            .header(reqwest::header::USER_AGENT, USER_AGENT);
        req = match std::env::var_os(GITHUB_AUTH_TOKEN) {
            Some(t_osstr) => {
                match t_osstr.into_string() {
                    Ok(t_str) => req.bearer_auth(t_str),
                    Err(_) => req,
                }
            },
            None => req
        };
        req.send().map_err(|e| SyscallProcessorError::Http(e))
    }

    /// Send a HTTP POST request only if an authentication token is present
    fn http_post(&self, sc_req: &syscalls::GithubRest, method: reqwest::Method) -> Result<reqwest::blocking::Response, SyscallProcessorError> {
        // GITHUB_REST_ENDPOINT is guaranteed to be parsable so unwrap is safe here
        let mut url = reqwest::Url::parse(GITHUB_REST_ENDPOINT).unwrap();
        url.set_path(&sc_req.route);
        match std::env::var_os(GITHUB_AUTH_TOKEN) {
            Some(t_osstr) => {
                match t_osstr.into_string() {
                    Ok(t_str) => {
                        self.http_client.request(method, url)
                            .header(reqwest::header::ACCEPT, GITHUB_REST_API_VERSION_HEADER)
                            .header(reqwest::header::USER_AGENT, USER_AGENT)
                            .body(std::string::String::from(sc_req.body.as_ref().unwrap()))
                            .bearer_auth(t_str)
                            .send().map_err(|e| SyscallProcessorError::Http(e))
                    },
                    Err(_) => Err(SyscallProcessorError::HttpAuth),
                }
            }
            None => Err(SyscallProcessorError::HttpAuth),
        }
    }

    pub fn run(mut self, env: &mut SyscallGlobalEnv, payload: String, s: &mut impl SyscallChannel) -> Result<TaskReturn, SyscallProcessorError> {
        use prost::Message;
        s.send(syscalls::Request{ payload }.encode_to_vec())?;

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
                Some(SC::Invoke(i)) => {
                    let result = match env.sched_conn.as_mut() {
                        None => {
                            warn!("No scheduler presents. Syscall invoke is noop.");
                            syscalls::WriteKeyResponse { success: false }
                        }
                        Some(sched_conn) => {
                            let ret = fs::utils::invoke(&env.fs, &i.gate).ok();
                            let result = syscalls::WriteKeyResponse { success: ret.is_some() };
                            if ret.is_some() {
                                let ret = ret.unwrap();
                                let label = fs::utils::get_current_label();
                                let sched_invoke = sched::message::LabeledInvoke {
                                    name: ret.0,
                                    payload: i.payload,
                                    gate_privilege: component_to_pbcomponent(&ret.1),
                                    label: Some(buckle_to_pblabel(&label)),
                                    sync: false,
                                };
                                sched::rpc::labeled_invoke(sched_conn, sched_invoke).map_err(|e| {
                                    error!("{:?}", e);
                                    SyscallProcessorError::UnreachableScheduler
                                })?;
                            }
                            result
                        }
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsDelete(req)) => {
                    let result = syscalls::WriteKeyResponse {
                        success: fs::utils::delete(&env.fs, &req.base_dir, req.name).is_ok(),
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
                            clause.principals.first_mut().unwrap().tokens.extend(suffix.tokens);
                        }
                    }
                    let result = syscalls::Buckle {
                        secrecy: my_priv,
                        integrity: None,
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::DupGate(req)) => {
                    let value = fs::utils::read_path(&env.fs, &req.orig).ok().and_then(|entry| { match entry {
                            fs::DirEntry::Gate(gate) => {
                                let policy = pblabel_to_buckle(&req.policy.unwrap());
                                fs::utils::create_gate(&env.fs, &req.base_dir, req.name, policy, gate.image).ok()
                            },
                            _ => None,
                        }
                    });
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::ReadKey(rk)) => {
                    let txn = DBENV.begin_ro_txn().unwrap();
                    let result = syscalls::ReadKeyResponse {
                        value: txn.get(env.db, &rk.key).ok().map(Vec::from),
                    };
                    let _ = txn.commit();
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::WriteKey(wk)) => {
                    let mut txn = DBENV.begin_rw_txn().unwrap();
                    let result = syscalls::WriteKeyResponse {
                        success: txn
                            .put(env.db, &wk.key, &wk.value, WriteFlags::empty())
                            .is_ok(),
                    };
                    let _ = txn.commit();
                    s.send(result.encode_to_vec())?;
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
                        let mut cursor = txn.open_ro_cursor(env.db).or(Err(SyscallProcessorError::Database))?.iter_from(&dir);
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
                    };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FsRead(req)) => {
                    let value = fs::utils::read(&env.fs, &req.path).ok();
                    let result = syscalls::ReadKeyResponse {
                        value
                    };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FsList(req)) => {
                    let value = fs::utils::list(&env.fs, &req.path).ok()
                        .map(|m| syscalls::EntryNameArr { names: m.keys().cloned().collect() });
                    let result = syscalls::FsListResponse { value };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FsFacetedList(req)) => {
                    let value = fs::utils::faceted_list(&env.fs, &req.path).ok()
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
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FsWrite(req)) => {
                    let value = fs::utils::write(&mut env.fs, &req.path, req.data).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FsCreateFacetedDir(req)) => {
                    let value = fs::utils::create_faceted(&env.fs, &req.base_dir, req.name).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsCreateDir(req)) => {
                    let label = pblabel_to_buckle(&req.label.clone().expect("label"));
                    let value = fs::utils::create_directory(&env.fs, &req.base_dir, req.name, label).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FsCreateFile(req)) => {
                    let label = pblabel_to_buckle(&req.label.clone().expect("label"));
                    let value = fs::utils::create_file(&env.fs, &req.base_dir, req.name, label).ok();
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some()
                    };
                    s.send(result.encode_to_vec())?;
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
                                let mut file = env.blobstore.create().map_err(|e| SyscallProcessorError::Blob(e))?;
                                let mut buf = [0; 4096];
                                while let Ok(len) = resp.read(&mut buf) {
                                    if len == 0 {
                                        break;
                                    }
                                    let _ = file.write_all(&buf[0..len]);
                                }
                                let result = env.blobstore.save(file).map_err(|e| SyscallProcessorError::Blob(e))?;
                                syscalls::GithubRestResponse {
                                    status: resp.status().as_u16() as u32,
                                    data: Vec::from(result.name),
                                }
                            } else {
                                syscalls::GithubRestResponse {
                                    status: resp.status().as_u16() as u32,
                                    data: resp.bytes().map_err(|e| SyscallProcessorError::Http(e))?.to_vec(),
                                }
                            }
                        },
                    };
                    s.send(result.encode_to_vec())?;
                },
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
                    let result = syscalls::DeclassifyResponse{
                        label: fs::utils::declassify(target).map(|l| buckle_to_pblabel(&l)).ok(),
                    };
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::CreateBlob(_cb)) => {
                    if let Ok(newblob) = env.blobstore.create().map_err(|e| SyscallProcessorError::Blob(e)) {
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
                    s.send(result.encode_to_vec())?;
                },
                Some(SC::FinalizeBlob(fb)) => {
                    let result = if let Some(mut newblob) = self.create_blobs.remove(&fb.fd) {
                        let blob = newblob.write_all(&fb.data)
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
                },
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
                },
                Some(SC::ReadBlob(rb)) => {
                    let result = if let Some(file) = self.blobs.get_mut(&rb.fd) {
                        let mut buf = Vec::from([0; 4096]);
                        let limit = std::cmp::min(rb.length.unwrap_or(4096), 4096) as usize;
                        if let Some(offset) = rb.offset {
                            file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| SyscallProcessorError::Blob(e))?;
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
                    s.send(result.encode_to_vec())?;
                },
                None => {
                    // Should never happen, so just ignore??
                },
            }
        }
    }
}
