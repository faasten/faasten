//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::result::Result;
use std::sync::mpsc::{Receiver};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::sync::atomic::{Ordering};
use std::os::unix::net::UnixListener;

use log::{error, warn, info, trace};
use time::precise_time_ns;

use crate::controller::Controller;
use crate::controller;
use crate::message::Message;
use crate::request::Request;
use crate::vm;
use crate::vm::Vm;
use crate::metrics::Metrics;
use crate::request;
//use crate::vsock::VsockStream;

const EVICTION_TIMEOUT: Duration = Duration::from_secs(2);
const MACPREFIX: &str = "FF:FF:FF:FF";

#[derive(Debug)]
pub struct Worker {
    pub thread: JoinHandle<()>,
    vm_listener: UnixListener,
}

#[derive(Debug)]
pub enum State {
    WaitForMsg,
    Shutdown,
    Response,
    ReqFail,
}

impl Worker {
    pub fn new(
        receiver: Arc<Mutex<Receiver<Message>>>,
        ctr: Arc<Controller>,
        cid: u32,
    ) -> Worker {
        // unlink unix listeners
        unsafe {
            let paths = vec![
                format!("worker-{}.sock_1234", cid),
                format!("worker-{}.sock", cid)
            ];
            for path in paths {
                // ignore errors
                let _ = libc::unlink(path.as_ptr() as *const libc::c_char);
            }
        }
        let vm_listener = match UnixListener::bind(format!("worker-{}.sock_1234", cid)) {
            Ok(listener) => listener,
            Err(e) => panic!("Failed to bind to unix listener \"worker-{}.sock_1234\": {:?}", cid, e),
        };
        let vm_listener_dup = match vm_listener.try_clone() {
            Ok(listener) => listener,
            Err(e) => panic!("Failed to clone unix listener \"worker-{}.sock_1234\": {:?}", cid, e),
        };

        let network = format!("tap{}/{}:{:02X}:{:02X}", (cid-100), MACPREFIX, ((cid-100)&0xff00)>>8, (cid-100) & 0xff);
        let handle = thread::spawn(move || {
            let id = thread::current().id();
            let mut stat: Metrics = Metrics::new();
            loop {
                let msg: Message = receiver.lock().unwrap().recv().unwrap();
                trace!("[Worker {:?}] task received: {:?}", id, msg);

                match msg {
                    // To shutdown, dump collected statistics and then terminate
                    Message::Shutdown => {
                        warn!("[Worker {:?}] shutdown received", id);
                        let output_file = std::fs::File::create(format!("out/thread-{:?}.stat", id))
                                              .expect("output file failed to create");
                        if let Err(e) = serde_json::to_writer_pretty(output_file, &stat.to_json()) {
                            error!("failed to write measurement results as json: {:?}", e);
                        }
                        return;
                    }
                    Message::Request(req, rsp_sender) => {
                        let function_name = req.function.clone();
                        let ret = Worker::acquire_vm(&function_name, &ctr, &mut stat, &vm_listener_dup, cid, &network)
                                  .and_then(|vm| {
                                        Worker::process_req(req, vm, &mut stat)
                                  });

                        match ret {
                            Ok(rsp) => {
                                trace!("[Worker {:?}] finished processing {}", id, function_name);
                                if let Err(e) = rsp_sender.send(Message::Response(rsp)) {
                                    error!("[Worker {:?}] response failed to send: {:?}", id, e);
                                }
                            }
                            Err(e) => match e {
                                controller::Error::InsufficientEvict |
                                controller::Error::LowMemory(_) => {
                                    warn!("[Worker {:?}] Resource exhaustion", id);
                                    stat.num_drop+=1;
                                }
                                controller::Error::FunctionNotExist=> {
                                    warn!("[Worker {:?}] Requested function doesn't exist: {:?}", id, function_name);
                                }
                                controller::Error::StartVm(vme) => {
                                    error!("[Worker {:?}] Failed to start vm due to: {:?}", id, vme);
                                    stat.num_vm_startfail+=1;
                                }
                                controller::Error::VmReqProcess(vme) => {
                                    error!("[Worker {:?}] Vm failed to process request due to: {:?}", id, vme);
                                    match vme {
                                        vm::Error::VmRead(_) => {
                                            stat.num_rsp_readfail+=1;
                                        }
                                        vm::Error::VmWrite(_) => {
                                            stat.num_req_writefail+=1;
                                        }
                                        _ => ()
                                    }
                                }
                                _ => {
                                    error!("[Worker {:?}] Unexpected controller error: {:?}", id, e);
                                }
                            }
                        }
                    }
                    Message::RequestTcp(req, rsp_sender) => {
                        let function_name = req.function.clone();
                        let ret = Worker::acquire_vm(&function_name, &ctr, &mut stat, &vm_listener_dup, cid, &network)
                                  .and_then(|vm| {
                                        Worker::process_req(req, vm, &mut stat)
                                  });

                        match ret {
                            Ok(rsp) => {
                                trace!("[Worker {:?}] finished processing {}", id, function_name);
                                {
                                    let mut s = rsp_sender.lock().expect("rsp_sender lock poisoned");
                                    if let Err(e) = request::write_u8(rsp.as_bytes(), &mut s) {
                                        error!("[Worker: {:?}] response failed to send: {:?}", id, e);
                                    }
                                }
                            }
                            Err(e) => {
                                match e {
                                    controller::Error::InsufficientEvict |
                                    controller::Error::LowMemory(_) => {
                                        warn!("[Worker {:?}] Resource exhaustion", id);
                                        stat.num_drop+=1;
                                    }
                                    controller::Error::FunctionNotExist=> {
                                        warn!("[Worker {:?}] Requested function doesn't exist: {:?}", id, function_name);
                                    }
                                    controller::Error::StartVm(vme) => {
                                        error!("[Worker {:?}] Failed to start vm due to: {:?}", id, vme);
                                        stat.num_vm_startfail+=1;
                                    }
                                    controller::Error::VmReqProcess(vme) => {
                                        error!("[Worker {:?}] Vm failed to process request due to: {:?}", id, vme);
                                        match vme {
                                            vm::Error::VmRead(_) => {
                                                stat.num_rsp_readfail+=1;
                                            }
                                            vm::Error::VmWrite(_) => {
                                                stat.num_req_writefail+=1;
                                            }
                                            _ => ()
                                        }
                                    }
                                    _ => {
                                        error!("[Worker {:?}] Unexpected controller error: {:?}", id, e);
                                    }
                                }
                                // Return a error message
                                {
                                    let err_msg = "Request failed";
                                    let mut s = rsp_sender.lock().expect("lock poisoned");
                                    if let Err(e) = request::write_u8(err_msg.as_bytes(), &mut s) {
                                        error!("[thread: {:?}] response failed to send: {:?}", id, e);
                                        println!("Failed to send request: {:?}", e);
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        error!("[Worker {:?}] Invalid message: {:?}", id, msg);
                    }
                }
            }
        });

        Worker { thread: handle, vm_listener }
    }

    pub fn process_req(req: Request, mut vm: Vm, stat: &mut Metrics) -> Result<String, controller::Error> {
        let t1 = precise_time_ns();
        let rsp = vm.process_req(req).map_err(|e| controller::Error::VmReqProcess(e));
        let t2 = precise_time_ns();

        if let Ok(_) = rsp {
            stat.num_complete = stat.num_complete + 1;
            stat.req_rsp_tsp.entry(vm.id).or_insert(vec![]).append(&mut vec![t1,t2]);
        }

        return rsp;
    }
    /// Send a request to a Vm, wait for Vm's response and return the result.
    /// A worker will first try to acquire an idle Vm to handle the request.
    /// If there are no idle Vms for the particular request, it will try to
    /// allocate a new Vm. If there's not enough resources on the machine to
    /// allocate a new Vm, it will try to evict an idle Vm from another
    /// function's idle list, and then allocate a new Vm.
    /// After processing the request, the worker will push its Vm into the idle
    /// list of its function.
    pub fn acquire_vm(
        function_name: &str,
        ctr: &Arc<Controller>,
        stat: &mut Metrics,
        vm_listener: &UnixListener,
        cid: u32,
        network: &str,
    )-> Result<Vm, controller::Error> {
        let thread_id = thread::current().id();
        let func_config = ctr.get_function_config(function_name).ok_or(controller::Error::FunctionNotExist)?;

        //let func_config = ctr.get_function_config(&function_name).ok_or_else(|| {
        //    error!("[Worker {:?}] Unknown request: {:?}", thread_id, req);
        //    Err(controller::Error::FunctionNotExist)
        //})?;

        // try to fail quickly here when there's resource exhaustion, i.e., when there's no idle vm
        // for this request, not enough memory to create a new VM, and not enough idle resources to
        // evict to create a new VM, we want this sequence of functions to return quickly and the
        // worker to return "Resource exhaustion" quickly.
        let vm = ctr
            .get_idle_vm(&function_name)
            .or_else(|e| {
                match e {
                   // No Idle vm for this function. Try to allocatea new vm.
                    controller::Error::NoIdleVm => {
                        let t1 = precise_time_ns();
                        let ret = ctr.allocate(func_config, vm_listener, cid, network);
                        let t2 = precise_time_ns();

                        if let Ok(vm) = ret.as_ref() {
                            let id = vm.id;
                            let mem_size = vm.memory;
                            stat.vm_mem_size.insert(id, mem_size);
                            stat.boot_tsp.insert(id, vec![t1, t2]);
                            info!("[Worker {:?}] Allocated new VM. ID: {:?}, App: {:?}", thread_id, id, func_config.name);
                        }
                        return ret;
                    }
                    // Just forward all other errors to the next `or_else`.
                    _ => {
                        return Err(e);
                    }
                }
            })
            .or_else(|e| {
                match e {
                    // Not enough free memory to allocate. Try eviction
                    controller::Error::LowMemory(_) => {
                        let mut freed: usize = 0;
                        let start = Instant::now();

                        while freed < func_config.memory && start.elapsed() < EVICTION_TIMEOUT {
                            if let Ok(mut vm) = ctr.find_evict_candidate(&function_name) {
                                info!("evicting vm: {:?}", vm);
                                let t1 = precise_time_ns();
                                vm.shutdown();
                                let t2 = precise_time_ns();

                                ctr.free_mem.fetch_add(vm.memory, Ordering::Relaxed);
                                freed = freed + vm.memory;

                                stat.evict_tsp.insert(vm.id, vec![t1, t2]);
                                stat.num_evict = stat.num_evict + 1;
                            }
                        }

                        // If we've freed up enough memory, try allocating again
                        // It's possible that even though we've freed up enough resources,
                        // we still can't allocate a VM because there are other workers
                        // running in parallel who might have grabbed the resources we
                        // freed and used it for their requests.
                        if freed >= func_config.memory {
                            let t1 = precise_time_ns();
                            let ret = ctr.allocate(func_config, vm_listener, cid, network);
                            let t2 = precise_time_ns();
                            if let Ok(vm) = ret.as_ref() {
                                let id = vm.id;
                                let mem_size = vm.memory;
                                stat.vm_mem_size.insert(id, mem_size);
                                stat.boot_tsp.insert(id, vec![t1, t2]);
                            }
                            return ret;
                        } else {
                            return Err(controller::Error::InsufficientEvict);
                        }
                    }
                    // Just return all other errors
                    _ => return Err(e)
                }
            });

        return vm;
    }
}
