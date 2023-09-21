///! secure runtime that holds the handles to the VM and the global file system
use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;

use labeled::Label;
use labeled::buckle::{Buckle, Component};
use crate::blobstore::{self, Blobstore, Blob};
use crate::fs::{self, BackingStore, FS, DirEntry, FsError, CURRENT_LABEL, Function, DirectGate, RedirectGate, Service, Gate};
use crate::sched::{self, message};
use crate::sched::message::{ReturnCode, TaskReturn};
use crate::syscalls::DentInvoke;
use crate::syscalls::{self, syscall::Syscall as SC};

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

pub struct SyscallProcessor<'a, B: BackingStore> {
    env: &'a mut SyscallGlobalEnv<B>,
    create_blobs: HashMap<u64, blobstore::NewBlob>,
    blobs: HashMap<u64, blobstore::Blob>,
    dents: HashMap<u64, fs::DirEntry>,
    max_blob_id: u64,
    max_dent_id: u64,
    http_client: reqwest::blocking::Client,
}

impl<'a, B: BackingStore + 'a> SyscallProcessor<'a, B> {
    pub fn new(env: &'a mut SyscallGlobalEnv<B>, label: Buckle, privilege: Component) -> Self {
        {
            // set up label & privilege
            fs::utils::clear_label();
            fs::utils::taint_with_label(label);
            fs::utils::set_my_privilge(privilege);
        }

        let mut dents: HashMap<u64, fs::DirEntry> = Default::default();
        dents.insert(0, DirEntry::Directory(fs::ROOT_REF));

        Self {
            env,
            create_blobs: Default::default(),
            blobs: Default::default(),
            dents,
            max_dent_id: 1,
            max_blob_id: 1,
            http_client: reqwest::blocking::Client::new(),
        }
    }

    pub fn new_insecure(env: &'a mut SyscallGlobalEnv<B>) -> Self {
        Self {
            env,
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
        service_info: &fs::Service,
        body: Option<Vec<u8>>,
        parameters: HashMap<String, String>
    ) -> Result<reqwest::blocking::Response, SyscallProcessorError> {
        let url = strfmt::strfmt(&service_info.url, &parameters).map_err(|_| SyscallProcessorError::BadUrlArgs)?;
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
}

impl<'a, B: BackingStore + 'a> SyscallProcessor<'a, B> {
    fn root(&self) -> syscalls::DentResult {
        syscalls::DentResult { success: true, fd: None, data: None }
    }

    fn dent_open(&mut self, dir_fd: u64, entry: syscalls::dent_open::Entry) -> syscalls::DentOpenResult {
        let result: Option<(u64, syscalls::DentKind)> = self.dents.get(&dir_fd).cloned().and_then(|base| match (base, entry) {
            (DirEntry::Directory(base_dir), syscalls::dent_open::Entry::Name(name)) => {
                base_dir.list(&self.env.fs).get(&name).map(|dent| {
                    let res_id = self.max_dent_id;
                    let _ = self.dents.insert(self.max_dent_id, dent.clone());
                    self.max_dent_id += 1;
                    (res_id, dent.into())
                })
            },
            (DirEntry::FacetedDirectory(base_dir), syscalls::dent_open::Entry::Facet(label)) => {
                let dent = DirEntry::Directory(base_dir.open(&label.into(), &self.env.fs));
                let res_id = self.max_dent_id;
                let _ = self.dents.insert(self.max_dent_id, dent.clone());
                self.max_dent_id += 1;
                Some((res_id, syscalls::DentKind::DentDirectory))
            },
            (DirEntry::FacetedDirectory(base_dir), syscalls::dent_open::Entry::Name(label_name)) => {
                if let Ok(label) = Buckle::parse(label_name.as_str()) {
                    let dent = DirEntry::Directory(base_dir.open(&label, &self.env.fs));
                    let res_id = self.max_dent_id;
                    let _ = self.dents.insert(self.max_dent_id, dent.clone());
                    self.max_dent_id += 1;
                    Some((res_id, syscalls::DentKind::DentDirectory))
                } else {
                    None
                }
            },
            _ => None,
        });
        if let Some(result) = result {
            syscalls::DentOpenResult { success: true, fd: result.0, kind: result.1.into() }
        } else {
            syscalls::DentOpenResult { success: false, fd: 0, kind: syscalls::DentKind::DentDirectory.into() }
        }
    }

    fn dent_close(&mut self, fd: u64) {
        syscalls::DentResult { success: self.dents.remove(&fd).is_some(), fd: None, data: None };
    }

    fn dent_create(&mut self, kind: syscalls::dent_create::Kind, label: Option<Buckle>) -> Result<syscalls::DentResult, FsError> {
        use syscalls::dent_create::Kind;
        let label = label.unwrap_or(Buckle::public());
        let entry: DirEntry = match kind {
            Kind::Directory(syscalls::Void {}) => {
                self.env.fs.create_directory(label)
            },
            Kind::File(syscalls::Void {}) => {
                self.env.fs.create_file(label)
            },
            Kind::FacetedDirectory(syscalls::Void {}) => {
                self.env.fs.create_faceted_directory()
            },
            Kind::Gate(syscalls::Gate { kind }) => {
                if let Some(kind) = kind {
                    match kind {
                        syscalls::gate::Kind::Direct(dg) => {
                            let function = dg.function.unwrap();

                            let DirEntry::Blob(app_image) = self.dents.get(&function.app_image).ok_or(FsError::InvalidFd)? else { Err(FsError::NotABlob)? };
                            let DirEntry::Blob(runtime_image) = self.dents.get(&function.runtime).ok_or(FsError::InvalidFd)? else { Err(FsError::NotABlob)? };
                            let DirEntry::Blob(kernel) = self.dents.get(&function.kernel).ok_or(FsError::InvalidFd)? else { Err(FsError::NotABlob)? };



                            let func = Function {
                                memory: function.memory as usize,
                                app_image: app_image.get(&self.env.fs).unwrap().unlabel().clone(),
                                runtime_image: runtime_image.get(&self.env.fs).unwrap().unlabel().clone(),
                                kernel: kernel.get(&self.env.fs).unwrap().unlabel().clone(),
                            };
                            self.env.fs.create_direct_gate(
                                label,
                                DirectGate {
                                    privilege: dg.privilege.unwrap().into(),
                                    invoker_integrity_clearance: dg.invoker_integrity_clearance.unwrap().into(),
                                    declassify: dg.declassify.map(|d| d.into()).unwrap_or(Component::dc_true()),
                                    function: func,
                                })?
                        },
                        syscalls::gate::Kind::Redirect(rd) => {
                            if let Some(DirEntry::Gate(gate_objref)) = self.dents.get(&rd.gate) {
                                self.env.fs.create_redirect_gate(
                                    label,
                                    RedirectGate {
                                        privilege: rd.privilege.unwrap().into(),
                                        invoker_integrity_clearance: rd.invoker_integrity_clearance.unwrap().into(),
                                        declassify: rd.declassify.map(|d| d.into()).unwrap_or(Component::dc_true()),
                                        gate: *gate_objref,
                                    })?
                            } else {
                                Err(FsError::NotAGate)?
                            }
                        }
                    }
                } else {
                    Err(FsError::NotAGate)?
                }
            },
            Kind::Service(syscalls::Service { taint, privilege, invoker_integrity_clearance, url, verb, mut headers }) => {
                let verb = syscalls::HttpVerb::from_i32(verb).unwrap_or(syscalls::HttpVerb::HttpHead).into();
                let headers: std::collections::BTreeMap<String, String> = headers.drain().collect();
                self.env.fs.create_service(
                    label,
                    Service {
                        taint: taint.unwrap().into(),
                        privilege: privilege.unwrap().into(),
                        invoker_integrity_clearance: invoker_integrity_clearance.unwrap().into(),
                        url,
                        verb,
                        headers
                    })?
            },
            Kind::Blob(blobfd) => {
                let blob = self.blobs.get(&blobfd).ok_or(FsError::NotABlob)?;
                self.env.fs.create_blob(label, blob.name.clone())?
            },
        };
        let res_id = self.max_dent_id;
        let _ = self.dents.insert(self.max_dent_id, entry);
        self.max_dent_id += 1;
        Ok(syscalls::DentResult { success: true, fd: Some(res_id), data: None })
    }

    fn dent_update(&mut self, fd: u64, kind: syscalls::dent_update::Kind) -> Result<syscalls::DentResult, FsError> {
        use syscalls::dent_update::Kind;
        match kind {
            Kind::File(data) => {
                if let Some(DirEntry::File(file)) = self.dents.get(&fd) {
                    file.write(data, &self.env.fs)?;
                } else {
                    return Err(FsError::NotAFile);
                }
            },
            Kind::Gate(syscalls::Gate { kind }) => {
                if let Some(DirEntry::Gate(gateentry)) = self.dents.get(&fd) {
                    if let Some(kind) = kind {
                        match kind {
                            syscalls::gate::Kind::Direct(dg) => {
                                let mut gate = if let Some(Gate::Direct(dg)) = gateentry.get(&self.env.fs).map(|e| e.unlabel().clone()) {
                                    dg
                                } else {
                                    return Err(FsError::NotAGate);
                                };
                                if let Some(function) = dg.function {

                                    if function.app_image > 0 {
                                        let DirEntry::Blob(app_image) = self.dents.get(&function.app_image).ok_or(FsError::InvalidFd)? else { Err(FsError::NotABlob)? };
                                        gate.function.app_image = app_image.get(&self.env.fs).unwrap().unlabel().clone();
                                    }
                                    if function.runtime > 0 {
                                        let DirEntry::Blob(runtime_image) = self.dents.get(&function.runtime).ok_or(FsError::InvalidFd)? else { Err(FsError::NotABlob)? };
                                        gate.function.runtime_image = runtime_image.get(&self.env.fs).unwrap().unlabel().clone();
                                    }

                                    if function.kernel > 0 {
                                        let DirEntry::Blob(kernel) = self.dents.get(&function.kernel).ok_or(FsError::InvalidFd)? else { Err(FsError::NotABlob)? };
                                        gate.function.kernel = kernel.get(&self.env.fs).unwrap().unlabel().clone();
                                    }

                                    if function.memory > 0 {
                                        gate.function.memory = function.memory as usize;
                                    }
                                }

                                if let Some(privilege) = dg.privilege {
                                    gate.privilege = privilege.into();
                                }

                                if let Some(invoker_integrity_clearance) = dg.invoker_integrity_clearance {
                                    gate.invoker_integrity_clearance = invoker_integrity_clearance.into();
                                }

                                gateentry.replace(Gate::Direct(gate), &self.env.fs)?;
                            },
                            syscalls::gate::Kind::Redirect(rd) => {
                                let mut gate = if let Some(Gate::Redirect(rg)) = gateentry.get(&self.env.fs).map(|e| e.unlabel().clone()) {
                                    rg
                                } else {
                                    return Err(FsError::NotAGate);
                                };

                                if let Some(DirEntry::Gate(gate_objref)) = self.dents.get(&rd.gate) {
                                    gate.gate = *gate_objref;
                                } else {
                                    return Err(FsError::NotAGate);
                                }

                                if let Some(privilege) = rd.privilege {
                                    gate.privilege = privilege.into();
                                }

                                if let Some(invoker_integrity_clearance) = rd.invoker_integrity_clearance {
                                    gate.invoker_integrity_clearance = invoker_integrity_clearance.into();
                                }

                                gateentry.replace(Gate::Redirect(gate), &self.env.fs)?
                            }
                        }
                    } else {
                        return Err(FsError::NotAGate);
                    }
                } else {
                    Err(FsError::NotAGate)?
                }
            },
            Kind::Service(syscalls::Service { taint, privilege, invoker_integrity_clearance, url, verb, mut headers }) => {
                if let Some(DirEntry::Service(service)) = self.dents.get(&fd) {
                    let verb = syscalls::HttpVerb::from_i32(verb).unwrap_or(syscalls::HttpVerb::HttpHead).into();
                    let headers: std::collections::BTreeMap<String, String> = headers.drain().collect();
                    service.replace(Service {
                            taint: taint.unwrap().into(),
                            privilege: privilege.unwrap().into(),
                            invoker_integrity_clearance: invoker_integrity_clearance.unwrap().into(),
                            url,
                            verb,
                            headers
                        }, &self.env.fs)?
                } else {
                    return Err(FsError::NotAService);
                }
            },
            Kind::Blob(blobfd) => {
                let blob = self.blobs.get(&blobfd).ok_or(FsError::NotABlob)?;
                if let Some(DirEntry::Blob(blobentry)) = self.dents.get(&fd) {
                    blobentry.replace(blob.name.clone(), &self.env.fs)?;
                } else {
                    return Err(FsError::NotABlob);
                }
            },
        };
        Ok(syscalls::DentResult { success: true, fd: None, data: None })
    }

    fn dent_read(&mut self, fd: u64) -> syscalls::DentResult {
        let result = self.dents.get(&fd).and_then(|entry| {
            match entry {
                DirEntry::File(file) => Ok(file.read(&self.env.fs)),
                _ => Err(FsError::NotAFile)
            }.ok()
        });
        syscalls::DentResult { success: result.is_some(), fd: Some(fd), data: result }
    }

    fn dent_list(&mut self, fd: u64) -> syscalls::DentListResult {
        let result = self.dents.get(&fd).and_then(|entry| match entry {
            DirEntry::Directory(dir) => Ok(
                dir.list(&self.env.fs).iter().map(|(name, direntry)| {
                    let kind = match direntry {
                        DirEntry::Directory(_) => syscalls::DentKind::DentDirectory,
                        DirEntry::File(_) => syscalls::DentKind::DentFile,
                        DirEntry::FacetedDirectory(_) => syscalls::DentKind::DentFacetedDirectory,
                        DirEntry::Gate(_) => syscalls::DentKind::DentGate,
                        DirEntry::Service(_) => syscalls::DentKind::DentService,
                        DirEntry::Blob(_) => syscalls::DentKind::DentBlob,
                    };
                    (name.clone(), kind as i32)
                }).collect()),
            _ => Err(FsError::NotADir),
        }.ok());
        if let Some(entries) = result {
            syscalls::DentListResult { success: true, entries }
        } else {
            syscalls::DentListResult { success: true, entries: Default::default() }
        }
    }

    fn dent_list_faceted(&mut self, fd: u64, clearance: Buckle) -> syscalls::DentLsFacetedResult {
        let result = self.dents.get(&fd).and_then(|entry| match entry {
            DirEntry::FacetedDirectory(faceted) => Ok(
                faceted.list(&self.env.fs, &clearance).iter().map(|(label, _)| {
                    label.clone().into()
                }).collect()),
            _ => Err(FsError::NotADir),
        }.ok());
        if let Some(facets) = result {
            syscalls::DentLsFacetedResult { success: true, facets }
        } else {
            syscalls::DentLsFacetedResult { success: false, facets: Default::default() }
        }
    }

    fn dent_ls_gate(&mut self, fd: u64) -> syscalls::DentLsGateResult {
        let result = self.dents.get(&fd).map(Clone::clone).and_then(|entry| match entry {
            DirEntry::Gate(gate) => Ok(
                match gate.get(&self.env.fs).unwrap().unlabel() {
                    fs::Gate::Direct(dg) => {
                        let app_image_fd = {
                            let blobid = self.max_blob_id;
                            self.max_blob_id += 1;
                            let blob = self.env.blobstore.open(dg.function.app_image.clone()).expect("open");
                            self.blobs.insert(blobid, blob);
                            blobid
                        };
                        let runtime_fd = {
                            let blobid = self.max_blob_id;
                            self.max_blob_id += 1;
                            let blob = self.env.blobstore.open(dg.function.runtime_image.clone()).expect("open");
                            self.blobs.insert(blobid, blob);
                            blobid
                        };
                        let kernel_fd = {
                            let blobid = self.max_blob_id;
                            self.max_blob_id += 1;
                            let blob = self.env.blobstore.open(dg.function.kernel.clone()).expect("open");
                            self.blobs.insert(blobid, blob);
                            blobid
                        };
                        let function = syscalls::Function {
                            memory: dg.function.memory as u64,
                            app_image: app_image_fd,
                            runtime: runtime_fd,
                            kernel: kernel_fd,
                        };
                        syscalls::Gate { kind: Some(syscalls::gate::Kind::Direct(syscalls::DirectGate {
                            privilege: Some(dg.privilege.clone().into()),
                            invoker_integrity_clearance: Some(dg.invoker_integrity_clearance.clone().into()),
                            declassify: Some(dg.declassify.clone().into()),
                            function: Some(function),
                        }))}
                    },
                    fs::Gate::Redirect(rd) => syscalls::Gate { kind: Some(syscalls::gate::Kind::Redirect(syscalls::RedirectGate {
                        privilege: Some(rd.privilege.clone().into()),
                        invoker_integrity_clearance: Some(rd.invoker_integrity_clearance.clone().into()),
                        declassify: Some(rd.declassify.clone().into()),
                        gate: 0, // unused field in this case
                    }))}
                }
            ),
            _ => Err(FsError::NotAGate),
        }.ok());
        syscalls::DentLsGateResult { success: result.is_some(), gate: result }
    }

    fn dent_link(&self, dir_fd: u64, name: String, target_fd: u64) -> syscalls::DentResult {
        let base_dir_m = self.dents.get(&dir_fd).cloned();
        let target_obj_m = self.dents.get(&target_fd).cloned();
        let result = base_dir_m.zip(target_obj_m).and_then(|(base, target)| match base {
            DirEntry::Directory(base_dir) => {
                base_dir.link(name, target, &self.env.fs).map_err(|e| {
                    Into::into(e)
                })
            },
            _ => {
                Err(FsError::NotADir)
            },
        }.ok());
        syscalls::DentResult { success: result.is_some(), fd: None, data: None }
    }

    fn dent_unlink(&self, fd: u64, name: &String) -> syscalls::DentResult {
        let result = self.dents.get(&fd).cloned().and_then(|entry| match entry {
            DirEntry::Directory(base_dir) => {
                base_dir.unlink(name, &self.env.fs).ok()
            },
            _ => None,
        });
        syscalls::DentResult { success: result.unwrap_or(false), fd: Some(fd), data: None }
    }

    fn dent_invoke(&mut self, fd: u64, payload: Vec<u8>, sync: bool, toblob: bool, parameters: HashMap<String, String>) -> syscalls::DentInvokeResult {
        let (blobfd, data, headers) = self.dents.get(&fd).cloned().and_then(|entry| match entry {
            DirEntry::Gate(gate) => {
                let gate = gate.to_invokable(&self.env.fs);
                if !crate::fs::utils::get_privilege().implies(&gate.invoker_integrity_clearance) {
                    return None;
                }
                sched::rpc::labeled_invoke(self.env.sched_conn.as_mut().unwrap(), sched::message::LabeledInvoke {
                    function: Some(gate.function.into()),
                    label: Some(CURRENT_LABEL.with(|cl| cl.borrow().clone()).into()),
                    gate_privilege: Some(gate.privilege.into()),
                    blobs: Default::default(),
                    payload,
                    headers: parameters,
                    sync,
                }).ok()?;
                if sync {
                    let res = message::read::<TaskReturn>(self.env.sched_conn.as_mut().unwrap()).ok()?;
                    let res_label = res.label.clone().map(Into::into).unwrap_or(Buckle::public());
                    fs::utils::taint_with_label(res_label);
                    if toblob {
                        // TODO(alevy): would be better to just pass this intent
                        // through the request and have the target just write a
                        // blob in the first place
                        let mut newblob = self.env.blobstore.create().expect("Create blob");
                        newblob.write_all(res.payload()).expect("Write to blob");
                        let blob = self.env.blobstore.save(newblob).expect("Save blob");
                        let blobfd = self.max_blob_id;
                        self.max_blob_id += 1;
                        self.blobs.insert(blobfd, blob);
                        Some((Some(blobfd), None, None))
                    } else {
                        Some((None, Some(res.payload().into()), None))
                    }
                } else {
                    Some((None, Some(vec![]), None))
                }
            },
            DirEntry::Service(service) => {
                let service_info = service.to_invokable(&self.env.fs);
                if !crate::fs::utils::get_privilege().implies(&service_info.invoker_integrity_clearance) {
                    return None;
                }
                crate::fs::utils::declassify_with(&service_info.privilege);
                let sendres = self.http_send(&service_info, Some(payload), parameters);
                crate::fs::utils::taint_with_label(service_info.taint);
                match sendres {
                    Ok(mut response) => {
                        let headers: HashMap<String, Vec<u8>> = response.headers().iter().map(|(a,b)| (a.to_string(), Vec::from(b.as_bytes()))).collect();
                        if toblob {
                            let mut newblob = self.env.blobstore.create().expect("Create blob");
                            response.copy_to(&mut newblob).expect("Copy to blob");
                            let blob = self.env.blobstore.save(newblob).expect("Save blob");
                            let blobfd = self.max_blob_id;
                            self.max_blob_id += 1;
                            self.blobs.insert(blobfd, blob);
                            Some((Some(blobfd), None, Some(headers)))
                        } else {
                            Some((None, response.bytes().map(|bs| bs.to_vec()).ok(), Some(headers)))
                        }
                    },
                    Err(_err) => {
                        None
                    }
                }
            }
            _ => None
        }).unwrap_or((None, None, None));

        syscalls::DentInvokeResult { success: blobfd.is_some() || data.is_some(), fd: blobfd, data, headers: headers.unwrap_or(Default::default()) }
    }

    fn dent_get_blob(&mut self, fd: u64) -> syscalls::BlobResult {
        match self.dents.get(&fd) {
            Some(DirEntry::Blob(blobentry)) => {
                let blob: Blob = self.env.blobstore.open(blobentry.read(&self.env.fs)).expect("blob");
                let blobfd = self.max_blob_id;
                self.max_blob_id += 1;
                let len = blob.len().expect("blob should exist");
                self.blobs.insert(blobfd, blob);
                syscalls::BlobResult { success: true, fd: blobfd, len, data: None }
            },
            _ => syscalls::BlobResult { success: false, fd: 0, len: 0, data: None }
        }
    }

    fn blob_create(&mut self) -> syscalls::BlobResult {
        match self.env.blobstore.create() {
            Ok(newblob) => {
                let blobid = self.max_blob_id;
                self.max_blob_id += 1;
                self.create_blobs.insert(blobid, newblob);
                syscalls::BlobResult { success: true, fd: blobid, len: 0, data: None }
            }
            Err(e) => {
                syscalls::BlobResult { success: false, fd: 0, len: 0, data: Some(e.to_string().into()) }
            }
        }
    }

    fn blob_write(&mut self, fd: u64, data: &[u8]) -> syscalls::BlobResult {
        if let Some(blob) = self.create_blobs.get_mut(&fd) {
            match blob.write(data) {
                Ok(len) => syscalls::BlobResult { success: true, fd, len: len as u64, data: None},
                Err(e) => syscalls::BlobResult { success: false, fd, len: 0, data: Some(e.to_string().into())}
            }
        } else {
            syscalls::BlobResult { success: false, fd, len: 0, data: None}
        }
    }

    fn blob_finalize(&mut self, fd: u64) -> syscalls::BlobResult {
        if let Some(blob) = self.create_blobs.remove(&fd) {
            let len = blob.len() as u64;
            match self.env.blobstore.save(blob) {
                Ok(blob) => {
                    self.blobs.insert(fd, blob);
                    syscalls::BlobResult { success: true, fd, len, data: None}
                }
                Err(e) => syscalls::BlobResult { success: false, fd, len, data: Some(e.to_string().into())}
            }
        } else {
            syscalls::BlobResult { success: false, fd, len: 0, data: None}
        }
    }

    fn blob_read(&mut self, fd: u64, offset: u64, length: u64) -> syscalls::BlobResult {
        if let Some(blob) = self.blobs.get(&fd) {
            let mut buf = vec![0; length as usize];
            match blob.read_at(&mut buf, offset) {
                Ok(len) => {
                    buf.resize(len, 0);
                    syscalls::BlobResult { success: true, fd, len: len as u64, data: Some(buf) }
                },
                Err(e) => syscalls::BlobResult { success: false, fd, len: 0, data: Some(e.to_string().into())}
            }
        } else {
            syscalls::BlobResult { success: false, fd, len: 0, data: None}
        }
    }

    fn blob_close(&mut self, fd: u64) -> syscalls::BlobResult {
        if self.blobs.remove(&fd).is_some() {
            syscalls::BlobResult { success: true, fd, len: 0, data: None}
        } else {
            syscalls::BlobResult { success: false, fd, len: 0, data: None}
        }
    }

    fn do_syscall(&mut self, sc: SC, s: &mut impl SyscallChannel) -> Result<Option<TaskReturn>, SyscallProcessorError> {
        use prost::Message;

        match sc {
            SC::Response(r) => {
                let result_label = fs::utils::declassify_with(&crate::fs::utils::get_privilege());
                return Ok(Some(TaskReturn {
                    code: ReturnCode::Success as i32,
                    payload: Some(r.payload),
                    label: Some(result_label.into()),
                }));
            },

            SC::BuckleParse(label) => {
                let result: Result<syscalls::Buckle, _> = Buckle::parse(label.as_str()).map(Into::into);
                s.send(syscalls::MaybeBuckle { label: result.ok() }.encode_to_vec())?;
            },
            SC::GetCurrentLabel(syscalls::Void {}) => {
                s.send(CURRENT_LABEL.with(|cl| syscalls::Buckle::from(cl.borrow().clone())).encode_to_vec())?;
            }
            SC::TaintWithLabel(label) => {
                s.send(CURRENT_LABEL.with(|cl| {
                    *cl.borrow_mut() = cl.borrow().clone().lub(label.into());
                    syscalls::Buckle::from(cl.borrow().clone())
                }).encode_to_vec())?;
            }
            SC::Declassify(component) => {
                let target = component.into();
                let result = syscalls::MaybeBuckle {
                    label: fs::utils::declassify(target)
                        .map(Into::into)
                        .ok(),
                };
                s.send(result.encode_to_vec())?;
            },
            SC::SubPrivilege(_) => todo!(),

            SC::Root(syscalls::Void {}) => {
                s.send(self.root().encode_to_vec())?
            }

            SC::DentOpen(syscalls::DentOpen { fd, entry }) => {
                s.send(self.dent_open(fd, entry.unwrap()).encode_to_vec())?;
            },
            SC::DentClose(fd) => {
                s.send(self.dent_close(fd).encode_to_vec())?;
            },
            SC::DentCreate(syscalls::DentCreate { kind, label }) => {
                let label = label.map(Into::into);
                s.send((if let Some(kind) = kind {
                    self.dent_create(kind, label).map_err(|e| log::info!("Err {:?}", e)).unwrap_or(
                        syscalls:: DentResult { success: false, fd: None, data: None }
                    )
                } else {
                    syscalls::DentResult { success: false, fd: None, data: None }
                }).encode_to_vec())?;
            },
            SC::DentUpdate(syscalls::DentUpdate { kind, fd }) => {
                s.send((if let Some(kind) = kind {
                    self.dent_update(fd, kind).map_err(|e| log::info!("Err {:?}", e)).unwrap_or(
                        syscalls:: DentResult { success: false, fd: None, data: None }
                    )
                } else {
                    syscalls::DentResult { success: false, fd: None, data: None }
                }).encode_to_vec())?;
            },
            SC::DentRead(fd) => {
                s.send(self.dent_read(fd).encode_to_vec())?
            },
            SC::DentList(fd) => {
                s.send(self.dent_list(fd).encode_to_vec())?
            },
            SC::DentLsFaceted(syscalls::DentLsFaceted { fd, clearance }) => {
                s.send(self.dent_list_faceted(fd, clearance.map(Into::into).unwrap_or(Buckle::public())).encode_to_vec())?
            },
            SC::DentLsGate(fd) => {
                s.send(self.dent_ls_gate(fd).encode_to_vec())?
            },
            SC::DentLink(syscalls::DentLink { dir_fd, name, target_fd }) => {
                s.send(self.dent_link(dir_fd, name, target_fd).encode_to_vec())?
            },
            SC::DentUnlink(syscalls::DentUnlink { fd, name }) => {
                s.send(self.dent_unlink(fd, &name).encode_to_vec())?
            },
            SC::DentInvoke(DentInvoke { fd, sync, payload, toblob, parameters }) => {
                s.send(self.dent_invoke(fd, payload, sync, toblob, parameters).encode_to_vec())?
            },
            SC::DentGetBlob(fd) => {
                s.send(self.dent_get_blob(fd).encode_to_vec())?
            },

            SC::BlobCreate(syscalls::BlobCreate { size: _ }) => {
                s.send(self.blob_create().encode_to_vec())?;
            }
            SC::BlobWrite(syscalls::BlobWrite { fd, data }) => {
                s.send(self.blob_write(fd, &data).encode_to_vec())?;
            }
            SC::BlobFinalize(syscalls::BlobFinalize { fd }) => {
                s.send(self.blob_finalize(fd).encode_to_vec())?;
            },
            SC::BlobRead(syscalls::BlobRead { fd, offset, length }) => {
                s.send(self.blob_read(fd, offset.unwrap_or(0), length.unwrap_or(4096)).encode_to_vec())?;
            },
            SC::BlobClose(syscalls::BlobClose { fd }) => {
                s.send(self.blob_close(fd).encode_to_vec())?;
            },
        };
        Ok(None)
    }

    pub fn run(
        mut self,
        payload: Vec<u8>,
        mut blobs: HashMap<String, Blob>,
        headers: HashMap<String, String>,
        s: &mut impl SyscallChannel,
    ) -> Result<TaskReturn, SyscallProcessorError> {
        use prost::Message;
        let blobfds = blobs.drain().map(|(k, b)| {
            let blobfd = self.max_blob_id;
            self.max_blob_id += 1;
            self.blobs.insert(blobfd, b);
            (k, blobfd)
        }).collect();
        s.send(syscalls::Request { payload, blobs: blobfds, headers }.encode_to_vec())?;

        loop {
            if let Some(sc) = s.wait()? {
                match self.do_syscall(sc, s) {
                    Err(er) => return Err(er),
                    Ok(Some(tr)) => return Ok(tr),
                    _ => {}

                }
            } else {
                // Should never reach here
            }
        }
    }
}
