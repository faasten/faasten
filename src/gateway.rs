use std::io::{BufReader, ErrorKind};
use std::fs::File;
use std::io::BufRead;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::{Mutex, Arc};
use std::collections::{VecDeque};
use std::thread;
use std::thread::JoinHandle;

use log::{error, warn, info};

use crate::message::Message;
use crate::request;

pub const DEFAULT_PORT:u32 = 28888;

/// A gateway listens on a endpoint and accepts requests
/// For example a FileGateway "listens" to a file and accepts
/// each line as request JSON string.
/// A HTTPGateway listens on a TCP port and accepts requests from
/// HTTP POST commands.
pub trait Gateway {
    fn listen(source: &str) -> std::io::Result<Self>
        where Self: std::marker::Sized;
}

#[derive(Debug)]
pub struct FileGateway {
    url: String,
    reader: BufReader<File>,
    rsp_sender: Sender<Message>,
    rsp_serializer: JoinHandle<()>,
}

impl Gateway for FileGateway {

    fn listen(source: &str) -> std::io::Result<Self>{
        match File::open(source) {
            Err(e) => Err(e),
            Ok(file) => {
                let reader = BufReader::new(file);

                let (tx, rx) = mpsc::channel();
                let handle = FileGateway::create_serializer_thread(rx);

                Ok(FileGateway {
                    url: source.to_string(),
                    reader: reader,
                    rsp_sender: tx,
                    rsp_serializer: handle,
                })
            }
        }
    }

}

impl FileGateway {
    fn create_serializer_thread(rx: Receiver<Message>) -> JoinHandle<()> {
        return thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(msg) => {
                        match msg {
                            Message::Response(rsp) => warn!("{:?}", rsp),
                            Message::Shutdown => return,
                            _ => error!("Reponse serializer received a non-response"),
                        }
                    },
                    Err(_) => () //TODO: handle errors
                }
            }
        });

    }

    pub fn shutdown(self) {
        &self.rsp_sender.send(Message::Shutdown);
        self.rsp_serializer.join().expect("Couldn't join on response serializer thread");
    }

}

impl Iterator for FileGateway {
    type Item = std::io::Result<(request::Request, Sender<Message>)>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = String::new();
        match self.reader.read_line(&mut buf) {
            Ok(0) => None,
            Ok(_n) => match request::parse_json(&buf) {
                Ok(req) => {
                    Some(Ok((req, self.rsp_sender.clone())))
                }
                Err(e) => Some(Err(std::io::Error::from(e)))
            }
            Err(e) => Some(Err(e))
        }
    }
}

#[derive(Debug)]
pub struct FileRequestIter {
    reader: BufReader<File>,
    rsp_sender: Sender<Message>,
}

impl Iterator for FileRequestIter {
    type Item = std::io::Result<(request::Request, Sender<Message>)>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = String::new();

        match self.reader.read_line(&mut buf) {
            Ok(0) => None,
            Ok(_n) => match request::parse_json(&buf) {
                Ok(req) => {
                    std::thread::sleep(std::time::Duration::from_millis(req.time));
                    Some(Ok((req, self.rsp_sender.clone())))
                }
                Err(e) => Some(Err(std::io::Error::from(e)))
            }
            Err(e) => Some(Err(e))
        }
    }
}


#[derive(Debug)]
pub struct HTTPGateway {
    pub port: u32,
    listener: JoinHandle<()>,
    pub streams: Arc<Mutex<VecDeque<Arc<Mutex<TcpStream>>>>>,
}

impl HTTPGateway {

    pub fn listen(port: &str) -> std::io::Result<Self> {
        let p: u32 = port.parse::<u32>().unwrap_or(DEFAULT_PORT);
        let addr = format!("localhost:{}", port);

        let streams: Arc<Mutex<VecDeque<Arc<Mutex<TcpStream>>>>> = Arc::new(Mutex::new(VecDeque::new()));
        let builder = thread::Builder::new();

        // create listener thread
        // A listener thread listens on `addr` for incoming TCP connections.
        let sc = streams.clone();
        let listener_handle = builder.spawn(move || {
            let listener = TcpListener::bind(addr).expect("listner failed to bind");
            for stream in listener.incoming() {
                if let Ok(stream) = stream {
                    stream.set_nonblocking(true).expect("cannot set stream to non-blocking");
                    {
                        let mut streams = sc.lock().expect("can't lock stream list");
                        streams.push_back(Arc::new(Mutex::new(stream)));
                        info!("number of streams: {:?}", streams.len());
                    }
                }

            }
        })?;

        Ok(HTTPGateway{
            port: p,
            listener: listener_handle,
            streams: streams,
        })

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
                        match request::parse_u8(buf) {
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
                                info!("connection {:?} closed by client", s);
                                return None;
                            }
                            // no data in the stream atm.
                            ErrorKind::WouldBlock => {
                            }
                            _ => {
                                // Some other error happened. Report and
                                // just try the next stream in the list
                                error!("Failed to read response: {:?}", e);
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

