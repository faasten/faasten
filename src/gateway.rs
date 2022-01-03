use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, Arc};
use std::collections::{VecDeque};
use std::thread::JoinHandle;

use log::{error, debug};

use crate::request;

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
    listener: JoinHandle<()>,
    streams: Arc<Mutex<VecDeque<Arc<Mutex<TcpStream>>>>>,
}

impl HTTPGateway {
    pub fn listen(addr: &str) -> Self {
        // create listener thread
        // A listener thread listens on `addr` for incoming TCP connections.
        let streams: Arc<Mutex<VecDeque<Arc<Mutex<TcpStream>>>>> = Arc::new(Mutex::new(VecDeque::new()));
        let sc = streams.clone();
        let listener = TcpListener::bind(addr).expect("listner failed to bind");
        debug!("Gateway started listening on: {:?}", addr);

        let listener_handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(stream) = stream {
                    debug!("connection from {:?}", stream.peer_addr());
                    stream.set_nonblocking(true).expect("cannot set stream to non-blocking");
                    {
                        let mut streams = sc.lock().expect("can't lock stream list");
                        streams.push_back(Arc::new(Mutex::new(stream)));
                    }
                }

            }
        });

        HTTPGateway{
            listener: listener_handle,
            streams: streams,
        }
    }
}

impl Iterator for HTTPGateway {
    type Item = std::io::Result<(request::Request, Arc<Mutex<TcpStream>>)>;

    fn next(&mut self) -> Option<Self::Item> {
        // For each TcpStream in a shared VecDeque of TcpStream values,
        // try to read a request from it.
        // If there's no data in the stream, move on to the next one.
        // If the stream returns EOF, close the stream and remove it
        // from the VecDeque.
        let s = self.streams.lock().expect("stream lock poisoned").pop_front();
        match s {
            // no connections
            None => {
                return None;
                //continue; // next() will block waiting for connections
            }
            Some(s) => {
                let res = request::read_u8(&mut s.lock().expect("lock failed"));
                match res {
                    // there's a request sitting in the stream
                    Ok(buf) => {
                        // If parse succeeds, return the Request value and a
                        // clone of the TcpStream value.
                        match request::parse_u8_request(buf) {
                            Err(e) => {
                                error!("request parsing failed: {:?}", e);
                            }
                            Ok(req) => {
                                //let stream_clone = s.try_clone().expect("cannot clone stream");
                                let c = s.clone();
                                self.streams.lock().expect("stream lock poisoned").push_back(s);
                                return Some(Ok((req, c)));
                            }
                        }
                    }
                    Err(e) => {
                        match e.kind() {
                            // when client closed the connection, remove the
                            // stream from stream list
                            ErrorKind::UnexpectedEof => {
                                debug!("connection {:?} closed by client", s);
                                return None;
                            }
                            // no data in the stream atm.
                            ErrorKind::WouldBlock => {
                            }
                            _ => {
                                // Some other error happened. Report and
                                // just try the next stream in the list
                                error!("Other error: {:?}", e);
                            }
                        }
                    }
                }

                self.streams.lock().expect("stream lock poisoned").push_back(s);
                return None;
            }
        }
    }
}

