//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::net::{SocketAddr, TcpStream};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};

use labeled::buckle::{Buckle, Component};
use labeled::Label;
use log::{debug, error};

use crate::configs::FunctionConfig;
use crate::vm::Vm;
//use crate::metrics::{self, WorkerMetrics};
use crate::fs::{self, BackingStore, Function, FS};
use crate::resource_manager;
use crate::sched::{
    self,
    message::{ReturnCode, TaskReturn},
};
use crate::syscall_server::*;

// one hour
//const FLUSH_INTERVAL_SECS: u64 = 3600;

#[derive(Debug)]
/// Manages VM allocation and boot process and communicates with the scheduler
pub struct Worker<B: BackingStore> {
    //pub thread: JoinHandle<()>,
    // each worker listens at the Unix socket worker-[cid].sock_1234
    cid: u32,
    thread_id: ThreadId,
    localrm: Arc<Mutex<resource_manager::ResourceManager>>,
    vm_listener: std::os::unix::net::UnixListener,
    //stat: WorkerMetrics,
    env: SyscallGlobalEnv<B>,
}

impl<B: BackingStore> Worker<B> {
    pub fn new(
        cid: u32,
        sched_addr: SocketAddr,
        localrm: Arc<Mutex<resource_manager::ResourceManager>>,
        backing_store: B,
    ) -> Self {
        let thread_id = thread::current().id();

        // connection to the scheduler
        let sched_conn = loop {
            debug!(
                "[Worker {:?}] trying to connect to the scheduler at {:?}",
                thread_id, sched_addr
            );
            if let Ok(conn) = TcpStream::connect(sched_addr) {
                break conn;
            }
            std::thread::sleep(std::time::Duration::new(5, 0));
        };
        debug!("[Worker{:?}] connected.", thread_id);

        // UNIX listener VMs connect to
        let vm_listener_path = format!("worker-{}.sock_1234", cid);
        let _ = std::fs::remove_file(&vm_listener_path);
        let vm_listener = UnixListener::bind(vm_listener_path).expect("bind to the Unix listener");

        // TODO what metrics do we want?
        // let _ = std::fs::create_dir_all("./out").unwrap();
        // let log_file = std::fs::File::create(format!("./out/thread-{:?}.stat", thread::current().id())).unwrap();
        //let stat = metrics::WorkerMetrics::new(log_file);
        //stat.start_timed_flush(FLUSH_INTERVAL_SECS);

        let default_fs = FS::new(backing_store);

        let env = SyscallGlobalEnv {
            sched_conn: Some(sched_conn),
            fs: default_fs,
            blobstore: Default::default(),
        };

        Self {
            cid,
            thread_id,
            localrm,
            vm_listener,
            /* stat, */ env,
        }
    }

    pub fn wait_and_process(&mut self) {
        use sched::message::response::Kind;
        loop {
            // rpc::get is blocking
            match sched::rpc::get(self.env.sched_conn.as_mut().unwrap()) {
                Err(e) => {
                    error!(
                        "[Worker {:?}] Failed to receive a scheduler response: {:?}",
                        self.thread_id, e
                    );
                    continue;
                }
                Ok(resp) => {
                    match resp.kind {
                        Some(Kind::Terminate(_)) => {
                            debug!("[Worker {:?}] terminate received", self.thread_id);
                            //self.stat.flush();
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
                            let label = invoke.label.unwrap().into();
                            let privilege: Component = invoke.gate_privilege.unwrap().into();
                            if let Some(mut vm) =
                                self.try_allocate(&invoke.function.unwrap().into(), &label)
                            {
                                let mut cnt = 0;
                                let mut ret = TaskReturn {
                                    code: ReturnCode::ProcessRequestFailed as i32,
                                    payload: None,
                                    label: Some(fs::utils::get_current_label().into()),
                                };
                                loop {
                                    cnt += 1;
                                    let mut config: FunctionConfig = vm.function.clone().into();
                                    config.kernel = self
                                        .env
                                        .blobstore
                                        .local_path_string(&vm.function.kernel)
                                        .unwrap_or_default();
                                    config.appfs = self
                                        .env
                                        .blobstore
                                        .local_path_string(&vm.function.app_image);
                                    config.runtimefs = self
                                        .env
                                        .blobstore
                                        .local_path_string(&vm.function.runtime_image)
                                        .unwrap_or_default();
                                    if let Err(e) = vm.launch(
                                        self.vm_listener.try_clone().unwrap(),
                                        self.cid,
                                        false,
                                        config,
                                        None,
                                    ) {
                                        error!(
                                            "[Worker {:?}] Failed VM launch: {:?}",
                                            self.thread_id, e
                                        );
                                        continue;
                                    }
                                    // TODO consider using meaningful clearance
                                    let blobs = invoke
                                        .blobs
                                        .iter()
                                        .map(|(k, b)| {
                                            (
                                                k.clone(),
                                                (self.env.blobstore.open(b.clone()).unwrap()),
                                            )
                                        })
                                        .collect();
                                    let processor = SyscallProcessor::new(
                                        &mut self.env,
                                        label.clone(),
                                        privilege.clone(),
                                    );
                                    if let Ok(result) = processor.run(
                                        invoke.payload.clone(),
                                        blobs,
                                        invoke.headers.clone(),
                                        invoke.invoker.clone().unwrap().into(),
                                        &mut vm,
                                    ) {
                                        ret = result;
                                        self.localrm.lock().unwrap().release(vm);
                                        break;
                                    }
                                    if cnt == 5 {
                                        if vm.handle.is_none() {
                                            ret.code = ReturnCode::LaunchFailed as i32;
                                        }
                                        self.localrm.lock().unwrap().delete(vm);
                                        break;
                                    }
                                }
                                if let Err(e) = sched::rpc::finish(
                                    &mut self.env.sched_conn.as_mut().unwrap(),
                                    task_id,
                                    ret,
                                ) {
                                    error!(
                                        "[Worker {:?}] Failed scheduler finish RPC: {:?}",
                                        self.thread_id, e
                                    );
                                }
                            } else {
                                let ret = TaskReturn {
                                    code: ReturnCode::ResourceExhausted as i32,
                                    payload: None,
                                    label: Some(fs::utils::get_current_label().into()),
                                };
                                if let Err(e) = sched::rpc::finish(
                                    &mut self.env.sched_conn.as_mut().unwrap(),
                                    task_id,
                                    ret,
                                ) {
                                    error!(
                                        "[Worker {:?}] Failed scheduler finish RPC: {:?}",
                                        self.thread_id, e
                                    );
                                };
                            }
                        }
                        _ => {
                            error!(
                                "[Worker {:?}] Unknown scheduler response: {:?}",
                                self.thread_id, resp
                            );
                            continue;
                        }
                    }
                }
            };
        }
    }

    fn try_allocate(&self, f: &Function, payload_label: &Buckle) -> Option<Vm> {
        let mut localrm = self.localrm.lock().unwrap();
        if let Some(vm) = localrm.get_cached_vm(f) {
            // cached VM must NOT be too tainted
            if vm.label.can_flow_to(payload_label) {
                return Some(vm);
            } else {
                localrm.release(vm);
            }
        }
        localrm.new_vm(f.clone())
    }
}
