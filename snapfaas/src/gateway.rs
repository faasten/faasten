use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver};

use log::{error, debug};

use crate::request;
use crate::metrics::RequestTimestamps;
// use crate::message::RequestInfo;

// TODO to suppress warming, remove this later
use std::sync::mpsc::Sender;
type RequestInfo = (request::Request, Sender<request::Response>, RequestTimestamps);

/// A gateway listens on a endpoint and accepts requests
/// For example a FileGateway "listens" to a file and accepts
/// each line as request JSON string.
/// A HTTPGateway listens on a TCP port and accepts requests from
/// HTTP POST commands.
pub trait Gateway {
    fn listen(source: &str) -> Self
        where Self: std::marker::Sized;
}

#[derive(Debug)]
pub struct HTTPGateway {
    requests: Receiver<RequestInfo>,
}

impl HTTPGateway {
    pub fn listen(addr: &str) -> Self {
        let listener = TcpListener::bind(addr).expect("listener failed to bind");
        debug!("Gateway started listening on: {:?}", addr);

        let (requests_tx, requests_rx) = channel();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    debug!("connection from {:?}", stream.peer_addr());
                    let requests = requests_tx.clone();
                    std::thread::spawn(move || {
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
                                    let (tx, rx) = channel::<request::Response>();
                                    let _ = requests.send((req, tx, timestamps));
                                    if let Ok(response) = rx.recv() {
                                        if let Err(e) = request::write_u8(&response.to_vec(), &mut stream) {
                                            error!("Failed to respond to TCP client at {:?}: {:?}", stream.peer_addr(), e);
    };
                                    }
                                }
                            }
                        }
                    });
                }
            }
        });

        HTTPGateway{
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

