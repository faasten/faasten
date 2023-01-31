//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;
use std::os::unix::net::UnixListener;

use labeled::Label;
use log::{error, debug};
use time::precise_time_ns;

use crate::fs::utils::get_current_label;
use crate::message::Message;
use crate::request::{RequestStatus, Response, LabeledInvoke};
use crate::vm;
use crate::metrics::{self, RequestTimestamps};
use crate::resource_manager;
use crate::fs;

// one hour
const FLUSH_INTERVAL_SECS: u64 = 3600;


#[derive(Debug)]
pub struct Worker {
    pub thread: JoinHandle<()>,
}

fn handle_request(req: LabeledInvoke, rsp_sender: Sender<Response>, func_req_sender: Sender<Message>, vm_req_sender: Sender<Message>, vm_listener: UnixListener, mut tsps: RequestTimestamps, stat: &mut metrics::WorkerMetrics, cid: u32) {
    debug!("invoke: {:?}", &req);

    tsps.arrived = precise_time_ns();

    fs::utils::clear_label();
    fs::utils::taint_with_label(labeled::buckle::Buckle::new(req.label.secrecy, true));
    fs::utils::set_my_privilge(req.gate.privilege);
    let function_name = req.gate.image;
    let mut i = 0;
    let result = loop {
        let mut tsps = tsps.clone();
        if i == 5 {
            break RequestStatus::ProcessRequestFailed;
        }
        i += 1;
        let (tx, rx) = mpsc::channel();
        vm_req_sender.send(Message::GetVm(function_name.clone(), tx)).expect("Failed to send GetVm request");
        match rx.recv().expect("Failed to receive GetVm response") {
            Ok(mut vm) => {
                tsps.allocated = precise_time_ns();
                if !vm.is_launched() {
                    // newly allocated VM is returned, launch it first
                    if let Err(e) = vm.launch(Some(func_req_sender.clone()), vm_listener.try_clone().expect("clone unix listener"), cid, false, None) {
                        handle_vm_error(e);
                        let _ = rsp_sender.send(Response {
                            status: RequestStatus::LaunchFailed,
                        });
                        // a VM launched or not occupies system resources, we need
                        // to put back the resources assigned to this VM.
                        vm_req_sender.send(Message::DeleteVm(vm)).expect("Failed to send DeleteVm request");
                        // insert the request's timestamps
                        stat.push(tsps);
                        continue;
                    }
                }
                if !vm.label.can_flow_to(&get_current_label()) {
                    debug!("Cached VM too tainted. Requesting new one.");
                    vm_req_sender.send(Message::ReleaseVm(vm)).expect("Failed to send ReleaseVm request");
                    let (tx, rx) = std::sync::mpsc::channel();
                    vm_req_sender.send(Message::NewVm(function_name.clone(), tx)).expect("Failed to send NewVm request");
                    if let Ok(newvm) = rx.recv().expect("Failed to receive NewVm response") {
                        vm = newvm;
                        if let Err(_) = vm.launch(Some(func_req_sender.clone()), vm_listener.try_clone().expect("clone unix listener"), cid, false, None) {
                            vm_req_sender.send(Message::DeleteVm(vm)).unwrap();
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                debug!("VM is launched");
                tsps.launched = precise_time_ns();

                match vm.process_req(req.payload.clone()) {
                    Ok(rsp) => {
                        tsps.completed = precise_time_ns();
                        // TODO: output are currently ignored
                        debug!("{:?}", rsp);
                        vm.label = fs::utils::get_current_label();
                        vm_req_sender.send(Message::ReleaseVm(vm)).expect("Failed to send ReleaseVm request");
                        break RequestStatus::SentToVM(rsp);
                    }
                    Err(e) => {
                        handle_vm_error(e);
                        vm_req_sender.send(Message::DeleteVm(vm)).expect("Failed to send DeleteVm request");
                        // insert the request's timestamps
                        stat.push(tsps);
                        continue;
                    },
                }

            },
            Err(e) => {
                // If VM allocation fails it is an unrecoverable error, no point in retrying.
                let id = thread::current().id();
                break match e {
                    resource_manager::Error::InsufficientEvict |
                    resource_manager::Error::LowMemory(_) => {
                        error!("[Worker {:?}] Resource exhaustion", id);
                        RequestStatus::ResourceExhausted
                    }
                    resource_manager::Error::FunctionNotExist=> {
                        error!("[Worker {:?}] Requested function doesn't exist: {:?}", id, function_name);
                        RequestStatus::FunctionNotExist
                    }
                    _ => {
                        error!("[Worker {:?}] Unexpected resource_manager error: {:?}", id, e);
                        RequestStatus::Dropped
                    }
                };
            }
        }
    };

    let _ = rsp_sender.send(Response {
        status: result
    });
    // insert the request's timestamps
    stat.push(tsps);

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
            std::fs::create_dir_all("./out").unwrap();
            let log_file = std::fs::File::create(format!("./out/thread-{:?}.stat", id)).unwrap();
            let mut stat = metrics::WorkerMetrics::new(log_file);
            stat.start_timed_flush(FLUSH_INTERVAL_SECS);

            let vm_listener_path = format!("worker-{}.sock_1234", cid);
            let _ = std::fs::remove_file(&vm_listener_path);
            let vm_listener = match UnixListener::bind(vm_listener_path) {
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
                        debug!("[Worker {:?}] shutdown received", id);
                        stat.flush();
                        return;
                    }
                    Message::Request((req, rsp_sender, tsps)) => {
                        handle_request(req, rsp_sender, func_req_sender.clone(), vm_req_sender.clone(), vm_listener_dup, tsps, &mut stat, cid)
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

fn handle_vm_error(vme: vm::Error) {
    let id = thread::current().id();
    match vme {
        vm::Error::ProcessSpawn(_) | vm::Error::VsockListen(_) =>
            error!("[Worker {:?}] Failed to start vm due to: {:?}", id, vme),
        vm::Error::VsockRead(_) | vm::Error::VsockWrite(_) =>
            error!("[Worker {:?}] Vm failed to process request due to: {:?}", id, vme),
        _ => (),
    }
}
