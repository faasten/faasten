///! secure runtime that holds the handles to the VM and the global file system
use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::net::TcpStream;

use labeled::buckle::{self, Buckle, Clause, Component};
use log::{debug, error, warn};

use crate::blobstore::{self, Blobstore};
use crate::fs::{self, BackingStore, FS};
use crate::sched::{
    self,
    message::{ReturnCode, TaskReturn},
};
use crate::syscalls::{self, syscall::Syscall as SC};

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
pub struct SyscallGlobalEnv<B: BackingStore> {
    pub sched_conn: Option<TcpStream>,
    pub fs: FS<B>,
    pub blobstore: Blobstore,
}

pub struct SyscallProcessor {
    create_blobs: HashMap<u64, blobstore::NewBlob>,
    blobs: HashMap<u64, blobstore::Blob>,
    dents: HashMap<u64, fs::DirEntry>,
    max_blob_id: u64,
    max_dent_id: u64,
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
            dents: Default::default(),
            max_blob_id: 0,
            max_dent_id: 0,
            http_client: reqwest::blocking::Client::new(),
        }
    }

    pub fn new_insecure() -> Self {
        Self {
            create_blobs: Default::default(),
            blobs: Default::default(),
            dents: Default::default(),
            max_blob_id: 0,
            max_dent_id: 0,
            http_client: reqwest::blocking::Client::new(),
        }
    }

    fn http_send(
        &self,
        service_info: &fs::ServiceInfo,
        body: Option<String>,
    ) -> Result<reqwest::blocking::Response, SyscallProcessorError> {
        let url = service_info.url.clone();
        let method = service_info.verb.clone().into();
        let headers = service_info
            .headers
            .iter()
            .map(|(a, b)| {
                (
                    reqwest::header::HeaderName::from_bytes(a.as_bytes()).unwrap(),
                    reqwest::header::HeaderValue::from_bytes(b.as_bytes()).unwrap(),
                )
            })
            .collect::<reqwest::header::HeaderMap>();
        let mut request = self.http_client.request(method, url).headers(headers);
        if let Some(body) = body {
            request = request.body(body);
        }
        request.send().map_err(|e| SyscallProcessorError::Http(e))
    }

    pub fn run<B: BackingStore>(
        mut self,
        env: &mut SyscallGlobalEnv<B>,
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
                Some(SC::InvokeService(req)) => {
                    let service = fs::utils::invoke_service(&env.fs, req.serv).ok();
                    let resp = match service {
                        Some(s) => {
                            fs::utils::taint_with_label(s.label.clone());
                            Some(self.http_send(&s, req.body)?)
                        }
                        None => None,
                    };
                    let result = match resp {
                        None => syscalls::ServiceResponse {
                            data: "Fail to invoke the external service".as_bytes().to_vec(),
                            status: 0,
                        },
                        Some(resp) => syscalls::ServiceResponse {
                            status: resp.status().as_u16() as u32,
                            data: resp
                                .bytes()
                                .map_err(|e| SyscallProcessorError::Http(e))?
                                .to_vec(),
                        },
                    };
                    s.send(result.encode_to_vec())?;
                }
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
                Some(SC::FsDupGate(dg)) => {
                    let mut success = false;
                    if let Ok(orig) = fs::path::Path::parse(&dg.orig) {
                        if let Ok(path) = fs::path::Path::parse(&dg.path) {
                            if let Some(base_dir) = path.parent() {
                                if let Some(name) = path.file_name() {
                                    if let Ok(policy) = buckle::Buckle::parse(&dg.policy) {
                                        success = fs::utils::dup_gate(
                                            &env.fs, orig, base_dir, name, policy,
                                        )
                                        .is_ok();
                                    }
                                }
                            }
                        }
                    }
                    let result = syscalls::WriteKeyResponse { success };
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
                                        if let Ok(url) =
                                            reqwest::Url::parse(&cs.url).map(|u| u.to_string())
                                        {
                                            if let Ok(verb) =
                                                reqwest::Method::from_bytes(cs.verb.as_bytes())
                                            {
                                                const DEFAULT_HEADERS: &[(&str, &str)] =
                                                    &[("ACCEPT", "*/*"), ("USER_AGENT", "faasten")];
                                                let headers = cs.headers.map_or_else(
                                                    || {
                                                        Ok(DEFAULT_HEADERS
                                                            .iter()
                                                            .map(|(n, v)| {
                                                                (n.to_string(), v.to_string())
                                                            })
                                                            .collect::<HashMap<_, _>>())
                                                    },
                                                    |hs| serde_json::from_str(&hs),
                                                );
                                                if let Ok(headers) = headers {
                                                    let value = fs::utils::create_service(
                                                        &env.fs, base_dir, name, policy, label,
                                                        url, verb, headers,
                                                    );
                                                    if value.is_err() {
                                                        debug!(
                                                            "fs-create-service failed: {:?}",
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
                            }
                        }
                    }
                    let result = syscalls::WriteKeyResponse { success };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsRead(rd)) => {
                    let value = fs::path::Path::parse(&rd.path)
                        .ok()
                        .and_then(|p| fs::utils::read(&env.fs, p).ok());
                    let result = syscalls::ReadKeyResponse { value };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::FsOpenBlob(rd)) => {
                    let value = fs::path::Path::parse(&rd.path)
                        .ok()
                        .and_then(|p| fs::utils::open_blob(&env.fs, p).ok());
                    let result = syscalls::FsOpenBlobResponse { name: value };
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
                Some(SC::FsHardLink(hl)) => {
                    let success = fs::path::Path::parse(&hl.src)
                        .ok()
                        .and_then(|src| {
                            fs::path::Path::parse(&hl.dest).ok().and_then(|dest| {
                                match fs::utils::read_path(&env.fs, src) {
                                    Ok(dent) => match dent {
                                        fs::DirEntry::FacetedDirectory(_)
                                        | fs::DirEntry::Directory(_)
                                        | fs::DirEntry::File(_)
                                        | fs::DirEntry::Blob(_) => Some(dent.clone()),
                                        _other_dent => None,
                                    },
                                    Err(_e) => None,
                                }
                                .and_then(|hard_link| {
                                    fs::utils::hard_link(&env.fs, dest, hard_link).ok()
                                })
                            })
                        })
                        .is_some();
                    let result = syscalls::WriteKeyResponse { success };
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
                                if let Some(l) = req.label {
                                    buckle::Buckle::parse(&l).ok().and_then(|l| {
                                        fs::utils::create_directory(&env.fs, base_dir, name, l).ok()
                                    })
                                } else {
                                    let l = fs::utils::get_current_label();
                                    fs::utils::create_directory(&env.fs, base_dir, name, l).ok()
                                }
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
                                if let Some(l) = req.label {
                                    buckle::Buckle::parse(&l).ok().and_then(|l| {
                                        fs::utils::create_file(&env.fs, base_dir, name, l).ok()
                                    })
                                } else {
                                    let l = fs::utils::get_current_label();
                                    fs::utils::create_file(&env.fs, base_dir, name, l).ok()
                                }
                            })
                        })
                    });
                    let result = syscalls::WriteKeyResponse {
                        success: value.is_some(),
                    };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::CreateFile(req)) => {
                    let value = fs::path::Path::parse(&req.path)
                        .ok()
                        .and_then(|p| {
                            p.file_name().and_then(|name| {
                                p.parent().and_then(|base_dir| {
                                    if let Some(l) = req.label.as_ref() {
                                        buckle::Buckle::parse(&l).ok().and_then(|l| {
                                            fs::utils::create_file(&env.fs, base_dir, name, l).ok()
                                        })
                                    } else {
                                        let l = fs::utils::get_current_label();
                                        fs::utils::create_file(&env.fs, base_dir, name, l).ok()
                                    }
                                })
                            })
                        })
                        .and_then(|dent| {
                            let _ = self.dents.insert(self.max_dent_id, dent);
                            self.max_dent_id += 1;
                            Some(self.max_dent_id - 1)
                        });
                    let result = value.map_or_else(
                        || syscalls::DentResponse {
                            success: false,
                            dent_fd: 0,
                        },
                        |fd| syscalls::DentResponse {
                            success: true,
                            dent_fd: fd,
                        },
                    );
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::DentWrite(r)) => {
                    let success = self
                        .dents
                        .get(&r.dent_fd)
                        .and_then(|dent| match dent {
                            fs::DirEntry::File(f) => env.fs.write(&f, &r.data).ok(),
                            _ => None,
                        })
                        .is_some();
                    let result = syscalls::WriteKeyResponse { success };
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::DentOpen(r)) => {
                    let value = self
                        .dents
                        .get(&r.dent_fd)
                        .and_then(|dent| match dent {
                            fs::DirEntry::Directory(dir) => {
                                env.fs.list(dir).ok().and_then(|l| l.get(&r.name).cloned())
                            }
                            _ => None,
                        })
                        .and_then(|dent| {
                            self.dents.insert(self.max_dent_id, dent);
                            self.max_dent_id += 1;
                            Some(self.max_dent_id - 1)
                        });
                    let result = value.map_or_else(
                        || syscalls::DentResponse {
                            success: false,
                            dent_fd: 0,
                        },
                        |fd| syscalls::DentResponse {
                            success: true,
                            dent_fd: fd,
                        },
                    );
                    s.send(result.encode_to_vec())?;
                }
                Some(SC::DentClose(r)) => {
                    // TODO garbage collector should correctly handle opened direntries
                    let success = self.dents.remove(&r.dent_fd).is_some();
                    let result = syscalls::WriteKeyResponse { success };
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
                Some(SC::Endorse(_en)) => {
                    let with_priv = _en.with_priv.map_or_else(
                        || fs::utils::my_privilege(),
                        |p| pbcomponent_to_component(&Some(p)),
                    );
                    fs::utils::endorse_with(&with_priv);
                    let result = syscalls::DeclassifyResponse {
                        label: Some(buckle_to_pblabel(&fs::utils::get_current_label())),
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
                        const MB: usize = 1024 * 1024;
                        let mut buf = Vec::from([0; 2 * MB]);
                        let limit = std::cmp::min(rb.length.unwrap_or(2 * MB as u64), 2 * MB as u64)
                            as usize;
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
