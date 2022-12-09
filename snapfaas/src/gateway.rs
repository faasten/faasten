use std::net::TcpListener;
use std::io::Write;
use std::sync::mpsc::{channel, Receiver};

use log::{error, debug};

use crate::message::RequestInfo;
use crate::request;

fn write_response(buf: &[u8], channel: &mut std::net::TcpStream) {
    let size = buf.len().to_be_bytes();
    if channel.write_all(&size).is_err() || channel.write_all(buf).is_err() {
        error!("Failed to respond");
    }
}

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
                        loop {
                            if let Ok(buf) = request::read_u8(&mut stream) {
                                if let Ok(parsed) = request::parse_u8_invoke(buf)  {
                                    use time::precise_time_ns;
                                    let timestamps = crate::metrics::RequestTimestamps {
                                        at_gateway: precise_time_ns(),
                                        ..Default::default()
                                    };
                                    let (tx, rx) = channel::<request::Response>();
                                    let _ = requests.send((parsed, tx, timestamps));
                                    if let Ok(response) = rx.recv() {
                                        write_response(&response.to_vec(), &mut stream);
                                    }

                                } else {
                                    write_response("Error decoding invoke".as_bytes(), &mut stream);
                                }
                            } else {
                                write_response("Error reading invoke".as_bytes(), &mut stream);
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

