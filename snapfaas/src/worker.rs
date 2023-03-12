//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::net::TcpStream;
use std::sync::mpsc::Sender;
use std::sync::mpsc;
use std::thread::{self, ThreadId};
use std::os::unix::net::UnixListener;

use labeled::Label;
use log::{error, debug};

use crate::message::Message;
use crate::vm::Vm;
use crate::metrics::{self, WorkerMetrics};
use crate::resource_manager;
use crate::fs::{self, FS};
use crate::labeled_fs::DBENV;
use crate::sched::{self, message::{TaskReturn, ReturnCode}};
use crate::syscall_server::*;

// one hour
const FLUSH_INTERVAL_SECS: u64 = 3600;

#[derive(Debug)]
/// Manages VM allocation and boot process and communicates with the scheduler
pub struct Worker {
    //pub thread: JoinHandle<()>,
    // each worker listens at the Unix socket worker-[cid].sock_1234
    cid: u32,
    thread_id: ThreadId,
    localrm_sender: Sender<Message>,
    vm_listener: tokio::net::UnixListener,
    stat: WorkerMetrics,
    env: SyscallGlobalEnv,
}

impl Worker {
    pub fn new(cid: u32, sched_addr: String, localrm_sender: Sender<Message>) -> Self {
        let thread_id = thread::current().id();

        // connection to the scheduler
        let sched_conn = TcpStream::connect(sched_addr).expect("failed to connect to the scheduler.");

        // UNIX listener VMs connect to
        let vm_listener_path = format!("worker-{}.sock_1234", cid);
        let _ = std::fs::remove_file(&vm_listener_path);
        let vm_listener = UnixListener::bind(vm_listener_path)
            .expect("bind to the Unix listener");
        let vm_listener = tokio::net::UnixListener::from_std(vm_listener)
            .expect("convert from UnixListener std");

        // stat (tentative)
        let _ = std::fs::create_dir_all("./out").unwrap();
        let log_file = std::fs::File::create(format!("./out/thread-{:?}.stat", thread::current().id())).unwrap();
        let stat = metrics::WorkerMetrics::new(log_file);
        stat.start_timed_flush(FLUSH_INTERVAL_SECS);

        let default_db = DBENV.open_db(None).expect("Cannot open the lmdb database");
        let default_fs = FS::new(&*DBENV);

        let env = SyscallGlobalEnv {
            sched_conn: Some(sched_conn),
            db: default_db,
            fs: default_fs,
            blobstore: Default::default(),
        };

        Self { cid, thread_id, localrm_sender, vm_listener, stat, env }
    }

    pub fn wait_and_process(&mut self) {
        use sched::message::response::Kind;
        loop {
            // rpc::get is blocking
            match sched::rpc::get(self.env.sched_conn.as_mut().unwrap()) {
                Err(e) => {
                    error!("[Worker {:?}] Failed to receive a scheduler response: {:?}", self.thread_id, e);
                    continue;
                }
                Ok(resp) => {
                    match resp.kind {
                        Some(Kind::Terminate(_)) => {
                            debug!("[Worker {:?}] terminate received", self.thread_id);
                            self.stat.flush();
                            return;
                        }
                        Some(Kind::ProcessTask(r)) => {
                            debug!("{:?}", r);
                            if r.labeled_invoke.is_none() {
                                error!("[Worker {:?}] labeled_invoke is None", self.thread_id);
                                continue;
                            }
                            let task_id = r.task_id;
                            let invoke = r.labeled_invoke.unwrap();
                            // must be called before try_allocate so we reject overly tainted
                            // cached VM.
                            let label = pblabel_to_buckle(invoke.label.as_ref().unwrap());
                            let privilege = pbcomponent_to_component(&invoke.gate_privilege);
                            let mut processor = SyscallProcessor::new(label.clone(), privilege.clone());
                            match self.try_allocate(&invoke.name) {
                                Ok(mut vm) => {
                                    let mut retry = 0;
                                    while retry < 5 {
                                        if let Err(e) = vm.launch(&self.vm_listener, self.cid, false, Default::default()) {
                                            error!("[Worker {:?}] Failed VM launch: {:?}", self.thread_id, e);
                                        } else if let Ok(result) = processor.run(&mut self.env, invoke.payload.clone(), &mut vm) {
                                            let _ = sched::rpc::finish(&mut self.env.sched_conn.as_mut().unwrap(), task_id.clone(), result);
                                            break;
                                        }
                                        retry += 1;
                                        processor = SyscallProcessor::new(label.clone(), privilege.clone());
                                    }
                                    if retry == 5 {
                                        let result = TaskReturn { code: ReturnCode::ProcessRequestFailed as i32, payload: None };
                                        if let Err(e) = sched::rpc::finish(&mut self.env.sched_conn.as_mut().unwrap(), task_id, result) {
                                            error!("[Worker {:?}] Failed scheduler finish RPC: {:?}", self.thread_id, e);
                                        }
                                    }
                                }
                                Err(resp) => {
                                    if let Err(e) = sched::rpc::finish(&mut self.env.sched_conn.as_mut().unwrap(), task_id, resp) {
                                        error!("[Worker {:?}] Failed scheduler finish RPC: {:?}", self.thread_id, e);
                                    };
                                }
                            }
                        }
                        _ => {
                            error!("[Worker {:?}] Unknown scheduler response: {:?}", self.thread_id, resp);
                            continue;
                        }
                    }
                }
            };
        }
    }

    fn try_allocate(&self, function: &str) -> Result<Vm, TaskReturn> {
        let map_rm_error_to_resp = |e: resource_manager::Error| -> TaskReturn {
            match e {
                resource_manager::Error::InsufficientEvict |
                resource_manager::Error::LowMemory(_) => {
                    error!("[Worker {:?}] Resource exhaustion", self.thread_id);
                    TaskReturn { code: ReturnCode::ResourceExhausted as i32, payload: None }
                }
                resource_manager::Error::FunctionNotExist=> {
                    // TODO this error should never happen once we move to invoker-side path
                    // resolution and self-hosting
                    error!("[Worker {:?}] Requested function doesn't exist: {:?}", self.thread_id, function);
                    TaskReturn { code: ReturnCode::FunctionNotExist as i32, payload: None }
                }
                _ => {
                    error!("[Worker {:?}] Unexpected resource_manager error: {:?}", self.thread_id, e);
                    TaskReturn { code: ReturnCode::Dropped as i32, payload: None }
                }
            }
        };

        let (tx, rx) = mpsc::channel();
        self.localrm_sender.send(Message::GetVm(function.to_string(), tx.clone())).expect("failed to send GetVm message");
        match rx.recv().expect("Failed to receive GetVm response") {
            Ok(vm) => {
                if !vm.label.can_flow_to(&fs::utils::get_current_label()) {
                    debug!("Cached VM too tainted. Requesting new one.");
                    self.localrm_sender.send(Message::ReleaseVm(vm)).expect("Failed to send ReleaseVm request");
                    self.localrm_sender.send(Message::NewVm(function.to_string(), tx)).expect("Failed to send NewVm request");
                    match rx.recv().expect("Failed to receive NewVm response") {
                        Ok(vm) => Ok(vm),
                        Err(e) => Err(map_rm_error_to_resp(e)),
                    }
                } else {
                    Ok(vm)
                }
            },
            Err(e) => Err(map_rm_error_to_resp(e)),
        }
    }
}
