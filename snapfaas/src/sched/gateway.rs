use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use log::{error, debug};

// use crate::request;
// use crate::message::RequestInfo;

use super::message;
use super::resource_manager::ResourceManager;
use super::rpc::ResourceInfo;
use super::RequestInfo;

pub type Manager = Arc<Mutex<ResourceManager>>;

/// A gateway listens on a endpoint and accepts requests
/// For example a FileGateway "listens" to a file and accepts
/// each line as request JSON string.
/// A HTTPGateway listens on a TCP port and accepts requests from
/// HTTP POST commands.
pub trait Gateway {
    fn listen(source: &str, manager: Option<Manager>) -> Self
        where Self: std::marker::Sized;
}

#[derive(Debug)]
pub struct HTTPGateway {
    requests: Receiver<RequestInfo>,
}

impl Gateway for HTTPGateway {
    fn listen(addr: &str, _manager: Option<Manager>) -> Self {
        let listener = TcpListener::bind(addr)
            .unwrap_or_else(|_| {
                panic!("listener failed to bind on {:?}", addr)
            });
        debug!("Gateway started listening on: {:?}", addr);
        let (requests_tx, requests_rx) = channel();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    debug!("connection from {:?}", stream.peer_addr());
                    let requests = requests_tx.clone();

                    std::thread::spawn(move || {
                        while let Ok(buf) = message::read_u8(&mut stream) {
                            if let Ok(parsed) = message::parse_u8_labeled_invoke(buf) {
                                // use time::precise_time_ns;
                                // let timestamps = crate::metrics::RequestTimestamps {
                                    // at_gateway: precise_time_ns(),
                                    // ..Default::default()
                                // };
                                let (tx, rx) = channel::<String>();
                                let _ = requests.send((parsed, tx));
                                if let Ok(response) = rx.recv() {
                                    let buf = response.as_bytes().to_vec();
                                    if message::write_u8(&mut stream, buf).is_err() {
                                        error!("Failed to write response");
                                    }
                                }
                            } else {
                                let buf = "Error decoding invoke".as_bytes().to_vec();
                                if message::write_u8(&mut stream, buf).is_err() {
                                    error!("Failed to write response");
                                }
                            }
                        }
                    });
                }
            }
        });

        HTTPGateway {
            requests: requests_rx,
        }
    }
}

impl Iterator for HTTPGateway {
    type Item = RequestInfo;

    fn next(&mut self) -> Option<Self::Item> {
        self.requests.recv().ok()
    }
}

#[derive(Debug)]
pub struct SchedGateway {
    rx: Arc<Mutex<Receiver<()>>>,
}

impl Gateway for SchedGateway {
    fn listen(addr: &str, manager: Option<Manager>) -> Self {
        let listener = TcpListener::bind(addr)
            .unwrap_or_else(|_| {
                panic!("listener failed to bind on {:?}", addr)
            });
        debug!("Gateway started listening on: {:?}", addr);

        let manager = manager.expect("No Resource Manager Found!");
        // to wait for resource before scheduling
        let (tx, rx) = channel();
        let rx = Arc::new(Mutex::new(rx));
        let rx_dup = Arc::clone(&rx);

        // handle incoming RPC requests
        thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    debug!("sched connection from {:?}", stream.peer_addr());
                    let manager = Arc::clone(&manager);
                    let tx = tx.clone();
                    let rx = Arc::clone(&rx_dup);

                    // process the RPC request
                    thread::spawn(move || {
                        while let Ok(req) = message::read_request(&mut stream) {
                            use message::{request::Kind, Response};
                            use super::Task;
                            match req.kind {
                                Some(Kind::GetTask(r)) => {
                                    debug!("RPC GET received {:?}", r.thread_id);
                                    let addr = stream.peer_addr().unwrap();
                                    let (task_sender, task_receiver) = channel();
                                    // release lock immediately because `schedule` will later
                                    // acquire it to send a task
                                    let _ = manager.lock().unwrap().add_idle(addr, task_sender);
                                    let _ = tx.send(()); // notify scheduler
                                    if let Ok(task) = task_receiver.recv() {
                                        match task {
                                            Task::Invoke(uuid, labeled_invoke) => {
                                                use message::response::Kind as ResKind;
                                                let res = message::Response {
                                                    kind: Some(ResKind::ProcessTask(message::ProcessTask {
                                                        task_id: uuid.to_string(),
                                                        labeled_invoke: Some(labeled_invoke),
                                                    })),
                                                };
                                                let _ = message::write(&mut stream, res);
                                            }
                                            Task::Terminate => {
                                                use message::response::Kind as ResKind;
                                                let res = Response {
                                                    kind: Some(ResKind::Terminate(message::Terminate {})),
                                                };
                                                let _ = message::write(&mut stream, res);
                                            }
                                        }
                                    }
                                }
                                Some(Kind::FinishTask(r)) => {
                                    debug!("RPC FINISH received {:?}", r.result);
                                    let res = Response { kind: None };
                                    let _ = message::write(&mut stream, res);
                                    let result = r.result;
                                    if let Ok(uuid) = uuid::Uuid::parse_str(&r.task_id) {
                                        if !uuid.is_nil() {
                                            let mut manager = manager.lock().unwrap();
                                            if let Some(tx) = manager.wait_list.remove(&uuid) {
                                                let _ = tx.send(result);
                                            }
                                        }
                                    }
                                }
                                Some(Kind::Invoke(r)) => {
                                    debug!("RPC INVOKE received {:?}", r.invoke);
                                    let _ = rx.lock().unwrap().recv();
                                    let manager_dup = Arc::clone(&manager);
                                    match message::parse_u8_labeled_invoke(r.invoke) {
                                        Ok(req) => {
                                            use super::schedule_async;
                                            thread::spawn(move || {
                                                let _ = schedule_async(req, manager_dup);
                                            });
                                            let res = Response { kind: None };
                                            let _ = message::write(&mut stream, res);
                                        }
                                        Err(_) => {
                                            // TODO return error message!
                                            let res = Response { kind: None };
                                            let _ = message::write(&mut stream, res);
                                        }
                                    }
                                }
                                Some(Kind::LabeledInvoke(r)) => {
                                    debug!("RPC INVOKE received {:?}", r);
                                    use super::schedule_async;
                                    let _ = rx.lock().unwrap().recv();
                                    let manager_dup = Arc::clone(&manager);
                                    thread::spawn(move || { // No check
                                        let _ = schedule_async(r, manager_dup);
                                    });
                                    let res = Response { kind: None };
                                    let _ = message::write(&mut stream, res);
                                }
                                Some(Kind::TerminateAll(_)) => {
                                    debug!("RPC TERMINATEALL received");
                                    let _ = manager.lock().unwrap().reset();
                                    let res = Response { kind: None };
                                    let _ = message::write(&mut stream, res);
                                    break;
                                }
                                Some(Kind::UpdateResource(r)) => {
                                    debug!("RPC UPDATE received");
                                    let manager = &mut manager.lock().unwrap();
                                    let info = serde_json::from_slice::<ResourceInfo>(&r.info);
                                    if let Ok(info) = info {
                                        let addr = stream.peer_addr().unwrap().ip();
                                        manager.update(addr, info);
                                        let res = Response { kind: None };
                                        let _ = message::write(&mut stream, res);
                                    } else {
                                        // TODO send error code
                                        let res = Response { kind: None };
                                        let _ = message::write(&mut stream, res);
                                    }
                                }
                                Some(Kind::DropResource(_)) => {
                                    debug!("RPC DROP received");
                                    let manager = &mut manager.lock().unwrap();
                                    let addr = stream.peer_addr().unwrap().ip();
                                    manager.remove(addr);
                                    let res = Response { kind: None };
                                    let _ = message::write(&mut stream, res);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                }
            }
        });

        SchedGateway { rx }
    }
}

impl Iterator for SchedGateway {
    type Item = ();

    fn next(&mut self) -> Option<Self::Item> {
        self.rx.lock().unwrap().recv().ok()
    }
}
