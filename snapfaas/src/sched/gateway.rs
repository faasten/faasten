// use std::net::TcpListener;

use std::sync::mpsc;
use log::{error, debug};

use crate::request;
use crate::metrics::RequestTimestamps;
// use crate::message::RequestInfo;
use crate::sched::{message, resource_manager};
use crate::sched::resource_manager::{ResourceManager, LocalResourceManagerInfo};

use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use prost::Message;

// FIXME tmp
type RequestInfo = (request::Request, RequestTimestamps);
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
    requests: mpsc::Receiver<RequestInfo>,
}

impl Gateway for HTTPGateway {
    fn listen(addr: &str, _manager: Option<Manager>) -> Self {
        let listener = TcpListener::bind(addr)
            .unwrap_or_else(|_| {
                panic!("listener failed to bind on {:?}", addr)
            });
        debug!("Gateway started listening on: {:?}", addr);
        let (requests_tx, requests_rx) = mpsc::channel();

        thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    debug!("connection from {:?}", stream.peer_addr());
                    let requests = requests_tx.clone();

                    thread::spawn(move || {
                        while let Ok(buf) = request::read_u8(&mut stream) {
                            // there's a request sitting in the stream
                            // If parse succeeds, return the Request value and a
                            // clone of the TcpStream value.
                            match request::parse_u8_request(buf) {
                                Err(e) => {
                                    error!("request parsing failed: {:?}", e);
                                    return;
                                }
                                Ok(req) => {
                                    use time::precise_time_ns;
                                    let timestamps = RequestTimestamps {
                                        at_gateway: precise_time_ns(),
                                        request: req.clone(),
                                        ..Default::default()
                                    };
                                    let _ = requests
                                        .send((req, timestamps));
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
    rx: mpsc::Receiver<()>,
}

impl Gateway for SchedGateway {
    fn listen(addr: &str, manager: Option<Manager>) -> Self {

        let listener = TcpListener::bind(addr)
            .unwrap_or_else(|_| {
                panic!("listener failed to bind on {:?}", addr)
            });
        debug!("Gateway started listening on: {:?}", addr);

        let manager = manager.expect("No Resource Manager Found!");
        // let manager_dup = Arc::clone(&manager);
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    debug!("sched connection from {:?}", stream.peer_addr());
                    let manager = Arc::clone(&manager);
                    let tx = tx.clone();

                    // process RPC request form stream
                    thread::spawn(move || {
                        use message::{Request, request::Kind, Response};
                        let req = message::recv_from(&mut stream)
                            .and_then(|b| {
                                let r = Request::decode(&b[..])?;
                                Ok(r)
                            });
                        let kind = req.ok().and_then(|r| r.kind);
                        match kind {
                            Some(Kind::Begin(id)) => {
                                debug!("sched worker ready {:?}", id);
                                let manager = &mut manager.lock().unwrap();
                                manager.add_idle(stream);
                                let _ = tx.send(());
                            }
                            Some(Kind::ShutdownAll(_)) => {
                                debug!("sched shutdown all");
                                let manager = &mut manager.lock().unwrap();
                                manager.reset();
                                let res = Response { kind: None }.encode_to_vec();
                                let _ = message::send_to(&mut stream, res);
                            }
                            Some(Kind::UpdateResource(buf)) => {
                                debug!("sched update resouce");
                                let manager = &mut manager.lock().unwrap();
                                let info = serde_json::from_slice::
                                            <LocalResourceManagerInfo>(&buf);
                                if let Ok(info) = info {
                                    let addr = stream.peer_addr().unwrap();
                                    manager.update(addr, info);
                                    let res = Response { kind: None }.encode_to_vec();
                                    let _ = message::send_to(&mut stream, res);
                                } else {
                                    // TODO send error code
                                    let res = Response { kind: None }.encode_to_vec();
                                    let _ = message::send_to(&mut stream, res);
                                }
                            }
                            _ => {}
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
        self.rx.recv().ok()
    }
}
