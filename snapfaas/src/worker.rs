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

use log::{error, debug};
use time::precise_time_ns;

use crate::message::Message;
use crate::request::{RequestStatus, Response, Request};
use crate::vm;
use crate::metrics::{self, RequestTimestamps};
use crate::resource_manager;
use crate::sched;

// one hour
const FLUSH_INTERVAL_SECS: u64 = 3600;


#[derive(Debug)]
pub struct Worker {
    pub thread: JoinHandle<()>,
}

fn handle_request(
    req: Request,
    // rsp_sender: Sender<Response>,
    // func_req_sender: Sender<Message>,
    vm_req_sender: Sender<Message>,
    vm_listener: UnixListener,
    // mut tsps: RequestTimestamps,
    stat: &mut metrics::WorkerMetrics,
    cid: u32,
    mock_github: Option<&str>
) -> Option<Vec<u8>> {
    debug!("processing request to function {}", &req.function);

    // tsps.arrived = precise_time_ns();

    let function_name = req.function.clone();
    let mut i = 0;
    let mut response = None;
    let result = loop {
        // let mut tsps = tsps.clone();
        if i == 5 {
            break RequestStatus::ProcessRequestFailed;
        }
        i += 1;
        let (tx, rx) = mpsc::channel();
        vm_req_sender.send(Message::GetVm(function_name.clone(), tx)).expect("Failed to send GetVm request");
        match rx.recv().expect("Failed to receive GetVm response") {
            Ok(mut vm) => {
                // tsps.allocated = precise_time_ns();
                if !vm.is_launched() {
                    // newly allocated VM is returned, launch it first
                    if let Err(e) = vm.launch(
                        None, // TODO invoke handle using scheduler
                        vm_listener.try_clone().expect("clone unix listener"),
                        cid, false,
                        None,
                        mock_github
                    ) {
                        handle_vm_error(e);

                        // TODO send response back the gateway
                        // let _ = rsp_sender.send(Response {
                            // status: RequestStatus::LaunchFailed,
                        // });

                        // a VM launched or not occupies system resources, we need
                        // to put back the resources assigned to this VM.
                        vm_req_sender.send(Message::DeleteVm(vm)).expect("Failed to send DeleteVm request");
                        // insert the request's timestamps
                        // stat.push(tsps);
                        continue;
                    }
                }

                debug!("VM is launched");
                // tsps.launched = precise_time_ns();

                match vm.process_req(req.clone()) {
                    Ok(rsp) => {
                        // tsps.completed = precise_time_ns();
                        // TODO: output are currently ignored
                        debug!("{:?}", rsp);
                        response = Some(rsp.as_bytes().to_vec());
                        vm_req_sender.send(Message::ReleaseVm(vm)).expect("Failed to send ReleaseVm request");
                        break RequestStatus::SentToVM(rsp);
                    }
                    Err(e) => {
                        handle_vm_error(e);
                        vm_req_sender.send(Message::DeleteVm(vm)).expect("Failed to send DeleteVm request");
                        // insert the request's timestamps
                        // stat.push(tsps);
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

    response

    // TODO send result back to the scheduler
    // let _ = rsp_sender.send(Response {
        // status: result
    // });

    // insert the request's timestamps
    // stat.push(tsps);
}

impl Worker {
    pub fn new(
        // receiver: Arc<Mutex<Receiver<Message>>>, // from worker pool
        vm_req_sender: Sender<Message>, // to local resource manager
        // func_req_sender: Sender<Message>, // to worker pool?
        cid: u32,
        mock_github: Option<String>,
        sched_addr: String,
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

                // TODO replace it to a RPC call to get the request
                // let msg: Message = receiver.lock().unwrap().recv().unwrap();

                let mut sched = sched::Scheduler::connect(&sched_addr);
                let message = sched.recv(); // wait for request
                let request = {
                    use sched::message::response::Kind;
                    use crate::request;
                    match message {
                        Ok(res) => {
                            match res.kind {
                                Some(Kind::Process(buf)) => {
                                    request::parse_u8_request(buf)
                                        .expect("Failed to parse request")
                                }
                                Some(Kind::Shutdown(_)) => {
                                    debug!("[Worker {:?}] shutdown received", id);
                                    stat.flush();
                                    return;
                                }
                                _ => continue,
                            }
                        }
                        Err(_) => continue,
                    }
                };

                let result = handle_request(request, //func_req_sender.clone(),
                    vm_req_sender.clone(),vm_listener_dup, &mut stat, cid,
                    mock_github.as_ref().map(String::as_str));

                let _ = sched.retn(result.unwrap_or(vec![]));



                // match msg {
                    // // To shutdown, dump collected statistics and then terminate
                    // Message::Shutdown => {
                        // debug!("[Worker {:?}] shutdown received", id);
                        // stat.flush();
                        // return;
                    // }
                    // Message::Request((req, tsps)) => {
                        // handle_request(req, rsp_sender, func_req_sender.clone(), vm_req_sender.clone(),
                            // vm_listener_dup, tsps, &mut stat, cid,
                            // mock_github.as_ref().map(String::as_str))
                    // }
                    // _ => {
                        // error!("[Worker {:?}] Invalid message: {:?}", id, msg);
                    // }
                // }
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
