//! Workers proxies requests and responses between the request manager and VMs.
//! Each worker runs in its own thread and is modeled as the following state
//! machine:
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

use labeled::buckle::Buckle;
use labeled::Label;
use log::{debug, error};
use r2d2::Pool;

use crate::configs::FunctionConfig;
use crate::vm::Vm;
//use crate::metrics::{self, WorkerMetrics};
use crate::fs::{Function, FS};
use crate::labeled_fs::DBENV;
use crate::resource_manager;
use crate::sched::{
    self,
    message::{ReturnCode, TaskReturn},
    Scheduler,
};
use crate::syscall_server::*;

// one hour
//const FLUSH_INTERVAL_SECS: u64 = 3600;

#[derive(Debug)]
/// Manages VM allocation and boot process and communicates with the scheduler
pub struct Worker {
    //pub thread: JoinHandle<()>,
    // each worker listens at the Unix socket worker-[cid].sock_1234
    cid: u32,
    thread_id: ThreadId,
    localrm: Arc<Mutex<resource_manager::ResourceManager>>,
    vm_listener: std::os::unix::net::UnixListener,
    //stat: WorkerMetrics,
    env: SyscallGlobalEnv,
    conn: Pool<Scheduler>,
}

impl Worker {
    pub fn new(
        cid: u32,
        conn: Pool<Scheduler>,
        localrm: Arc<Mutex<resource_manager::ResourceManager>>,
    ) -> Self {
        let thread_id = thread::current().id();

        // UNIX listener VMs connect to
        let vm_listener_path = format!("worker-{}.sock_1234", cid);
        let _ = std::fs::remove_file(&vm_listener_path);
        let vm_listener = UnixListener::bind(vm_listener_path).expect("bind to the Unix listener");

        // TODO what metrics do we want?
        // let _ = std::fs::create_dir_all("./out").unwrap();
        // let log_file = std::fs::File::create(format!("./out/thread-{:?}.stat", thread::current().id())).unwrap();
        //let stat = metrics::WorkerMetrics::new(log_file);
        //stat.start_timed_flush(FLUSH_INTERVAL_SECS);

        let default_db = DBENV.open_db(None).expect("Cannot open the lmdb database");
        let default_fs = FS::new(&*DBENV);

        let env = SyscallGlobalEnv {
            sched_conn: None,
            db: default_db,
            fs: default_fs,
            blobstore: Default::default(),
        };

        Self {
            cid,
            thread_id,
            localrm,
            vm_listener,
            /* stat, */
            env,
            conn,
        }
    }

    pub fn wait_and_process(&mut self) {
        use sched::message::response::Kind;
        loop {
            let maybe_conn = self.conn.get();
            if maybe_conn.is_err() {
                error!("{}", maybe_conn.unwrap_err().to_string());
                std::thread::sleep(Duration::new(1, 0));
                continue;
            }
            self.env.sched_conn = Some(maybe_conn.unwrap().try_clone().unwrap());

            // rpc::get is blocking
            match sched::rpc::get(self.env.sched_conn.as_mut().unwrap()) {
                Err(e) => {
                    error!(
                        "[Worker {:?}] Failed to receive a GET response: {:?}",
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
                            let label = pblabel_to_buckle(invoke.label.as_ref().unwrap());
                            let privilege = pbcomponent_to_component(&invoke.gate_privilege);
                            if let Some(mut vm) =
                                self.try_allocate(&invoke.function.unwrap().into(), &label)
                            {
                                let mut cnt = 0;
                                let mut ret = TaskReturn {
                                    code: ReturnCode::ProcessRequestFailed as i32,
                                    payload: None,
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
                                    let processor = SyscallProcessor::new(
                                        label.clone(),
                                        privilege.clone(),
                                        Buckle::top(),
                                    );
                                    if let Ok(result) = processor.run(
                                        &mut self.env,
                                        invoke.payload.clone(),
                                        &mut vm,
                                    ) {
                                        ret = result;
                                        self.localrm
                                            .lock()
                                            .unwrap()
                                            .release(vm, self.env.sched_conn.as_mut().unwrap());
                                        break;
                                    }
                                    if cnt == 5 {
                                        if vm.handle.is_none() {
                                            ret.code = ReturnCode::LaunchFailed as i32;
                                        }
                                        self.localrm
                                            .lock()
                                            .unwrap()
                                            .delete(vm, self.env.sched_conn.as_mut().unwrap());
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
                        Some(Kind::ProcessTaskInsecure(r)) => {
                            debug!("{:?}", r);
                            if r.unlabeled_invoke.is_none() {
                                error!("[Worker {:?}] labeled_invoke is None", self.thread_id);
                                continue;
                            }
                            let task_id = r.task_id;
                            let invoke = r.unlabeled_invoke.unwrap();
                            if let Some(mut vm) =
                                self.try_allocate_no_label_check(&invoke.function.unwrap().into())
                            {
                                let mut cnt = 0;
                                let mut ret = TaskReturn {
                                    code: ReturnCode::ProcessRequestFailed as i32,
                                    payload: None,
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
                                    let processor = SyscallProcessor::new_insecure();
                                    if let Ok(result) = processor.run(
                                        &mut self.env,
                                        invoke.payload.clone(),
                                        &mut vm,
                                    ) {
                                        ret = result;
                                        self.localrm
                                            .lock()
                                            .unwrap()
                                            .release(vm, self.env.sched_conn.as_mut().unwrap());
                                        break;
                                    }
                                    if cnt == 5 {
                                        if vm.handle.is_none() {
                                            ret.code = ReturnCode::LaunchFailed as i32;
                                        }
                                        self.localrm
                                            .lock()
                                            .unwrap()
                                            .delete(vm, self.env.sched_conn.as_mut().unwrap());
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

    fn try_allocate(&mut self, f: &Function, payload_label: &Buckle) -> Option<Vm> {
        if let Some(vm) = self
            .localrm
            .lock()
            .unwrap()
            .get_cached_vm(f, self.env.sched_conn.as_mut().unwrap())
        {
            // cached VM must NOT be too tainted
            if !vm.label.can_flow_to(payload_label) {
                return Some(vm);
            } else {
                self.localrm
                    .lock()
                    .unwrap()
                    .release(vm, self.env.sched_conn.as_mut().unwrap());
            }
        }
        self.localrm
            .lock()
            .unwrap()
            .new_vm(f.clone(), self.env.sched_conn.as_mut().unwrap())
    }

    fn try_allocate_no_label_check(&mut self, f: &Function) -> Option<Vm> {
        if let Some(vm) = self
            .localrm
            .lock()
            .unwrap()
            .get_cached_vm(f, self.env.sched_conn.as_mut().unwrap())
        {
            return Some(vm);
        }
        self.localrm
            .lock()
            .unwrap()
            .new_vm(f.clone(), self.env.sched_conn.as_mut().unwrap())
    }
}
