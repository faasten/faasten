use std::io::{BufReader};
use std::fs::File;
use std::io::BufRead;
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver};
use std::thread;
use std::thread::JoinHandle;

use log::{error, warn, info};

use crate::message::Message;
use crate::request;

pub const DEFAULT_PORT: &str = "8888";

/// A gateway listens on a endpoint and accepts requests
/// For example a FileGateway "listens" to a file and accepts
/// each line as request JSON string.
/// A HTTPGateway listens on a TCP port and accepts requests from
/// HTTP POST commands.
pub trait Gateway {
    type Iter: Iterator;
    fn listen(source: &str) -> std::io::Result<Self>
        where Self: std::marker::Sized;
    fn incoming(self) -> Self::Iter;
}

#[derive(Debug)]
pub struct FileGateway {
    url: String,
    reader: BufReader<File>,
    rsp_sender: Sender<Message>,
    rsp_serializer: JoinHandle<()>,
}

impl Gateway for FileGateway {
    type Iter = FileRequestIter;

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

    fn incoming(self) -> Self::Iter {
        return FileRequestIter {reader: self.reader, rsp_sender: self.rsp_sender.clone()};
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
        self.rsp_serializer.join();
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
            Ok(_n) => match request::parse_json(buf) {
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
    port: u32,
    listener: TcpListener,
}

