//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::result::Result;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;
use std::os::unix::net::UnixListener;

use log::{error, warn, debug};
use time::precise_time_ns;

use crate::message::Message;
use crate::request::{Request, RequestStatus, Response};
use crate::vm;
use crate::vm::Vm;
use crate::metrics::Metrics;
use crate::resource_manager;

#[derive(Debug)]
pub struct Worker {
    pub thread: JoinHandle<()>,
}

impl Worker {
    pub fn new(
        receiver: Arc<Mutex<Receiver<Message>>>,
        vm_req_sender: Sender<Message>,
        func_req_sender: Sender<Message>,
        cid: u32,
    ) -> Self {
        let handle = thread::spawn(move || {
            let id = thread::current().id();
            let mut stat: Metrics = Metrics::new();

            let vm_listener = match UnixListener::bind(format!("worker-{}.sock_1234", cid)) {
                Ok(listener) => listener,
                Err(e) => panic!("Failed to bind to unix listener \"worker-{}.sock_1234\": {:?}", cid, e),
            };

            loop {
                let vm_listener_dup = match vm_listener.try_clone() {
                    Ok(listener) => listener,
                    Err(e) => panic!("Failed to clone unix listener \"worker-{}.sock_1234\": {:?}", cid, e),
                };

                let msg: Message = receiver.lock().unwrap().recv().unwrap();
                match msg {
                    // To shutdown, dump collected statistics and then terminate
                    Message::Shutdown => {
                        warn!("[Worker {:?}] shutdown received", id);
                        if let Err(e) = std::fs::create_dir_all("./out") {
                            error!("Cannot create stats folder ./out: {:?}", e);
                        } else {
                            match std::fs::File::create(format!("out/thread-{:?}.stat", id)) {
                                Ok(output_file) =>
                                    if let Err(e) = serde_json::to_writer_pretty(output_file, &stat.to_json()) {
                                        error!("failed to write measurement results as json: {:?}", e);
                                    },
                                Err(e) => {
                                    error!("Cannot create stat file out/thread-{:?}.stat: {:?}", id, e);
                                },
                            }
                        }
                        return;
                    }
                    Message::Request(req, rsp_sender) => {
                        debug!("processing request to function {}", &req.function);
                        let function_name = req.function.clone();
                        let (tx, rx) = mpsc::channel();
                        vm_req_sender.send(Message::GetVm(function_name, tx)).expect("Failed to send GetVm request");
                        match rx.recv().expect("Failed to receive GetVm response") {
                            Ok(mut vm) => {
                                if !vm.is_launched() {
                                    // newly allocated VM is returned, launch it first
                                    if let Err(e) = vm.launch(Some(func_req_sender.clone()), vm_listener_dup, cid, false, None) {
                                        handle_vm_error(e, &mut stat);                                    
                                        let _ = rsp_sender.send(Response {
                                            status: RequestStatus::LaunchFailed,
                                        });
                                        // a VM launched or not occupies system resources, we need
                                        // to put back the resources assigned to this VM.
                                        vm_req_sender.send(Message::DeleteVm(vm)).expect("Failed to send DeleteVm request");
                                        continue;
                                    }
                                }

                                debug!("VM is launched");
                                let _ = rsp_sender.send(Response {
                                    status: RequestStatus::SentToVM,
                                });

                                match process_req(req, &mut vm, &mut stat) {
                                    Ok(rsp) => {
                                        // TODO: output are currently ignored
                                        debug!("{:?}", rsp);
                                    }
                                    Err(e) => handle_vm_error(e, &mut stat),
                                }

                                vm_req_sender.send(Message::ReleaseVm(vm)).expect("Failed to send ReleaseVm request");
                            },
                            Err(e) => {
                                let status = handle_resource_manager_error(e, &mut stat, &req.function);
                                let _ = rsp_sender.send(Response {
                                    status,
                                });
                            }
                        }
                    }
                    _ => {
                        error!("[Worker {:?}] Invalid message: {:?}", id, msg);
                    }
                }
            }
        });

        Worker { thread: handle }
    }

    pub fn join(self) -> std::thread::Result<()> {
        self.thread.join()
    }
}

fn process_req(req: Request, vm: &mut Vm, stat: &mut Metrics) -> Result<String, vm::Error> {
    let t1 = precise_time_ns();
    let rsp = vm.process_req(req.payload);
    let t2 = precise_time_ns();

    if let Ok(_) = rsp {
        stat.num_complete = stat.num_complete + 1;
        stat.req_rsp_tsp.entry(vm.id()).or_insert(vec![]).append(&mut vec![t1,t2]);
    }
    rsp
}

fn handle_resource_manager_error(e: resource_manager::Error, stat: &mut Metrics, function_name: &str) -> RequestStatus {
    let id = thread::current().id();
    match e {
        resource_manager::Error::InsufficientEvict |
        resource_manager::Error::LowMemory(_) => {
            warn!("[Worker {:?}] Resource exhaustion", id);
            stat.num_drop+=1;
            RequestStatus::ResourceExhausted
        }
        resource_manager::Error::FunctionNotExist=> {
            warn!("[Worker {:?}] Requested function doesn't exist: {:?}", id, function_name);
            RequestStatus::FunctionNotExist
        }
        _ => {
            error!("[Worker {:?}] Unexpected resource_manager error: {:?}", id, e);
            RequestStatus::Dropped
        }
    }
}

fn handle_vm_error(vme: vm::Error, stat: &mut Metrics) {
    let id = thread::current().id();
    match vme {
        vm::Error::ProcessSpawn(_) | vm::Error::VsockListen(_) => {
            error!("[Worker {:?}] Failed to start vm due to: {:?}", id, vme);
            stat.num_vm_startfail+=1;
        }
        _ => {
            error!("[Worker {:?}] Vm failed to process request due to: {:?}", id, vme);
            match vme {
                vm::Error::VsockRead(_) => {
                    stat.num_rsp_readfail+=1;
                }
                vm::Error::VsockWrite(_) => {
                    stat.num_req_writefail+=1;
                }
                _ => ()
            }
        }
    }
}
