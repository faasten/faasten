use core::panic;
use log::{debug, error, warn};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use super::message;
use super::resource_manager::ResourceManager;
use super::rpc::ResourceInfo;
use super::Task;

pub type Manager = Arc<Mutex<ResourceManager>>;

pub struct RpcServer {
    manager: Manager,
    listener: TcpListener,
    queue_tx: crossbeam::channel::Sender<Task>,
    cvar: Arc<Condvar>,
}

impl RpcServer {
    pub fn new(
        addr: &str,
        manager: Manager,
        queue_tx: crossbeam::channel::Sender<Task>,
        cvar: Arc<Condvar>,
    ) -> Self {
        Self {
            manager,
            listener: TcpListener::bind(addr).expect("bind to the TCP listening address"),
            queue_tx,
            cvar,
        }
    }

    pub fn run(self) {
        loop {
            for stream in self.listener.incoming() {
                if let Ok(stream) = stream {
                    debug!("connection from {:?}", stream.peer_addr());
                    let manager = Arc::clone(&self.manager);
                    let queue_tx = self.queue_tx.clone();
                    let cvar = self.cvar.clone();

                    thread::spawn(move || RpcServer::serve(stream, manager, queue_tx, cvar));
                }
            }
        }
    }

    // Process the RPC request
    fn serve(
        mut stream: TcpStream,
        manager: Manager,
        queue_tx: crossbeam::channel::Sender<Task>,
        cvar: Arc<Condvar>,
    ) {
        while let Ok(req) = message::read_request(&mut stream) {
            use message::{request::Kind, response::Kind as ResKind, Response};
            match req.kind {
                Some(Kind::Ping(_)) => {
                    debug!("PING");
                    let res = Response {
                        kind: Some(ResKind::Pong(message::Pong {})),
                    };
                    let _ = message::write(&mut stream, &res);
                }
                Some(Kind::GetTask(r)) => {
                    debug!("RPC GET from {:?}", r.thread_id);
                    manager
                        .lock()
                        .unwrap()
                        .add_idle(stream.peer_addr().unwrap(), stream.try_clone().unwrap());
                    cvar.notify_one();
                }
                Some(Kind::FinishTask(r)) => {
                    //let res = Response { kind: None };
                    //let _ = message::write(&mut stream, &res);
                    let result = r.result.unwrap();
                    debug!("RPC FINISH result {:?}", result);
                    if let Ok(uuid) = uuid::Uuid::parse_str(&r.task_id) {
                        if !uuid.is_nil() {
                            let mut manager = manager.lock().unwrap();
                            if let Some(mut conn) = manager.wait_list.remove(&uuid) {
                                let _ = message::write(&mut conn, &result);
                            }
                        }
                    }
                }
                Some(Kind::LabeledInvoke(r)) => {
                    debug!("RPC LABELED INVOKE received {:?}", r);
                    let uuid = uuid::Uuid::new_v4();
                    let sync = r.sync;
                    match queue_tx.try_send(Task::Invoke(uuid, r)) {
                        Err(crossbeam::channel::TrySendError::Full(_)) => {
                            warn!("Dropping Invocation from {:?}", stream.peer_addr());
                            let ret = message::TaskReturn {
                                code: message::ReturnCode::QueueFull as i32,
                                payload: None,
                            };
                            let _ = message::write(&mut stream, &ret);
                        }
                        Err(crossbeam::channel::TrySendError::Disconnected(_)) => {
                            panic!("Broken request queue")
                        }
                        Ok(()) => {
                            if sync {
                                manager
                                    .lock()
                                    .unwrap()
                                    .wait_list
                                    .insert(uuid, stream.try_clone().unwrap());
                            }
                        }
                    }
                }
                Some(Kind::UnlabeledInvoke(r)) => {
                    debug!("RPC UNLABELED INVOKE received {:?}", r);
                    let uuid = uuid::Uuid::new_v4();
                    match queue_tx.try_send(Task::InvokeInsecure(uuid, r)) {
                        Err(crossbeam::channel::TrySendError::Full(_)) => {
                            warn!("Dropping Invocation from {:?}", stream.peer_addr());
                            let ret = message::TaskReturn {
                                code: message::ReturnCode::QueueFull as i32,
                                payload: None,
                            };
                            let _ = message::write(&mut stream, &ret);
                        }
                        Err(crossbeam::channel::TrySendError::Disconnected(_)) => {
                            panic!("Broken request queue")
                        }
                        Ok(()) => (),
                    }
                }
                //Some(Kind::TerminateAll(_)) => {
                //    debug!("RPC TERMINATEALL received");
                //    let _ = manager.lock().unwrap().remove_all();
                //    let res = Response { kind: None };
                //    let _ = message::write(&mut stream, &res);
                //    break;
                //}
                Some(Kind::UpdateResource(r)) => {
                    debug!("RPC UPDATE received");
                    let manager = &mut manager.lock().unwrap();
                    let info = serde_json::from_slice::<ResourceInfo>(&r.info);
                    if let Ok(info) = info {
                        let addr = stream.peer_addr().unwrap().ip();
                        manager.update(addr, info);
                        //let res = Response { kind: None };
                        //let _ = message::write(&mut stream, &res);
                    } else {
                        // TODO Send error code
                        error!("Failed to deserialize ResourceInfo")
                        //let res = Response { kind: None };
                        //let _ = message::write(&mut stream, &res);
                    }
                    cvar.notify_one();
                }
                Some(Kind::DropResource(_)) => {
                    debug!("RPC DROP received");
                    let manager = &mut manager.lock().unwrap();
                    let addr = stream.peer_addr().unwrap().ip();
                    manager.remove(addr);
                    //let res = Response { kind: None };
                    //let _ = message::write(&mut stream, &res);
                    break;
                }
                _ => {}
            }
        }
        error!("Peer disconnected {:?}", stream.peer_addr());
    }
}
