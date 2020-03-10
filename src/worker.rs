//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::result::Result;
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering};

use log::{error, info};
use time::precise_time_ns;

use crate::controller::Controller;
use crate::message::Message;
use crate::request::Request;
use crate::vm::Vm;
use crate::metrics::Metrics;

const EVICTION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct Worker {
    pub thread: JoinHandle<()>,
}

#[derive(Debug)]
pub enum State {
    WaitForMsg,
    Shutdown,
    Response,
    ReqFail,
}

impl Worker {
    pub fn new(receiver: Arc<Mutex<Receiver<Message>>>, ctr: Arc<Controller>) -> Worker {
        let handle = thread::spawn(move || {
            let id = thread::current().id();
            let mut stat: Metrics = Metrics::new();
            loop {
                let msg: Message = receiver.lock().unwrap().recv().unwrap();
                info!("Thread {:?} task received: {:?}", id, msg);
                match msg {
                    Message::Shutdown => {
                        info!("Thread {:?} shutdown received", id);

                        let mut output_file = std::fs::File::create(format!("thread-{:?}.stat", id))
                                              .expect("output file failed to create");
                        if let Err(e) = serde_json::to_writer_pretty(output_file, &stat.to_json()) {
                            panic!("failed to write measurement results as json");
                        }
                        return;
                    }
                    Message::Request(req, rsp_sender) => match Worker::process_req(req, &ctr,
                        &mut stat) { Ok(rsp) => {
                            info!("Thread {:?} finished processing", id);
                            if let Err(e) = rsp_sender.send(Message::Response(rsp)) {
                                error!("[thread: {:?}] response failed to send: {:?}", id, e);
                            }
                        }
                        Err(err) => info!("Request failed: {:?}", err),
                    },
                    _ => {
                        error!("Invalid message to thread {:?}: {:?}", id, msg);
                    }
                }
            }
        });

        Worker { thread: handle }
    }

    /// Send a request to a Vm, wait for Vm's response and return the result.
    /// A worker will first try to acquire an idle Vm to handle the request.
    /// If there are no idle Vms for the particular request, it will try to
    /// allocate a new Vm. If there's not enough resources on the machine to
    /// allocate a new Vm, it will try to evict an idle Vm from another
    /// function's idle list, and then allocate a new Vm.
    /// After processing the request, the worker will push its Vm into the idle
    /// list of its function.
    pub fn process_req(req: Request, ctr: &Arc<Controller>, stat: &mut Metrics)
        -> Result<String, String> {
        let id = thread::current().id();

        let function_name = req.function.clone();
        let func_config = ctr.get_function_config(&function_name);
        if func_config.is_none() {
            error!("[Worker {:?}] Unknown request: {:?}", id, req);
            return Err(String::from("Unknown request"));
        }
        let func_config = func_config.unwrap(); // unwrap should be safe here

        //info!("[Worker {:?}] recv: {:?}", id, req);
        //info!("[Worker {:?}] function config: {:?}", id, func_config);

        // try to fail quickly here when there's resource exhaustion, i.e., when there's no idle vm
        // for this request, not enough memory to create a new VM, and not enough idle resources to
        // evict to create a new VM, we want this sequence of functions to return quickly and the
        // worker to return "Resource exhaustion" quickly.
        let vm = ctr
            .get_idle_vm(&function_name)
            .or_else(|| {
                let t1 = precise_time_ns();
                let vm = ctr.allocate(func_config);
                let t2 = precise_time_ns();
                if vm.is_some() {
                    let id = vm.as_ref().unwrap().id;
                    let mem_size = vm.as_ref().unwrap().memory;
                    stat.vm_mem_size.insert(id, mem_size);
                    stat.boot_tsp.insert(id, vec![t1, t2]);
                }
                vm
            })
            .or_else(|| {
                let mut freed: usize = 0;
                let start = Instant::now();

                while freed < func_config.memory && start.elapsed() < EVICTION_TIMEOUT {
                    if let Some(mut vm) = ctr.find_evict_candidate() {
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
                    let vm = ctr.allocate(func_config);
                    let t2 = precise_time_ns();
                    if vm.is_some() {
                        let id = vm.as_ref().unwrap().id;
                        let mem_size = vm.as_ref().unwrap().memory;
                        stat.vm_mem_size.insert(id, mem_size);
                        stat.boot_tsp.insert(id, vec![t1, t2]);
                    }
                    return vm;
                }
                return None;
            });

        return match vm {
            None => {
                {
                    stat.num_drop = stat.num_drop + 1;
                }
                Err(String::from("Resource exhaustion"))
            }
            Some(mut vm) => {
                let t1 = precise_time_ns();
                // send the request to VM and wait for a response
                let res = vm.process_req(req);
                let t2 = precise_time_ns();

                stat.num_complete = stat.num_complete + 1;
                stat.req_rsp_tsp.entry(vm.id).or_insert(vec![]).append(&mut vec![t1,t2]);

                ctr.release(&function_name, vm);

                res
            }
        };
    }
}
