use std::net::TcpListener;
use std::io::{Write, Read};
use std::sync::mpsc::{channel, Receiver};

use log::{error, debug};
use prost::Message;

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
                            let buf = {
                                let mut lenbuf = [0;4];
                                if stream.read_exact(&mut lenbuf).is_err() {
                                    write_response("Error reading size".as_bytes(), &mut stream);
                                }
                                let size = u32::from_be_bytes(lenbuf);
                                let mut buf = vec![0u8; size as usize];
                                if stream.read_exact(&mut buf).is_err() {
                                    write_response("Error reading invoke".as_bytes(), &mut stream);
                                }
                                buf
                            };

                            use crate::syscalls::Syscall;
                            use crate::syscalls::syscall::Syscall as SC;
                            match Syscall::decode(buf.as_ref()) {
                                Err(_) => write_response("Error decoing invoke".as_bytes(), &mut stream),
                                Ok(sc) => match sc.syscall {
                                    Some(SC::Invoke(req)) => {
                                        use time::precise_time_ns;
                                        let timestamps = crate::metrics::RequestTimestamps {
                                            at_gateway: precise_time_ns(),
                                            ..Default::default()
                                        };
                                        let (tx, rx) = channel::<request::Response>();
                                        let _ = requests.send((req, tx, timestamps));
                                        if let Ok(response) = rx.recv() {
                                            write_response(&response.to_vec(), &mut stream);
                                        }
                                    }
                                    _ => write_response("Not invoke".as_bytes(), &mut stream),
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

