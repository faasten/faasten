use std::io::{BufReader, Lines};
use std::fs::File;
use std::io::BufRead;
use std::net::TcpListener;
use snapfaas::request;

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
}

impl Gateway for FileGateway {
    type Iter = FileRequestIter;

    fn listen(source: &str) -> std::io::Result<Self>{
        match File::open(source) {
            Err(e) => Err(e),
            Ok(file) => {
                let reader = BufReader::new(file);
                Ok(FileGateway {
                    url: source.to_string(),
                    reader: reader,
                })
            }
        }
    }

    fn incoming(self) -> Self::Iter {
        return FileRequestIter {reader: self.reader};
    }
}

#[derive(Debug)]
pub struct FileRequestIter {
    reader: BufReader<File>
}

impl Iterator for FileRequestIter {
    type Item = std::io::Result<request::Request>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = String::new();
        match self.reader.read_line(&mut buf) {
            Ok(0) => None,
            Ok(_n) => match request::parse_json(buf) {
                Ok(req) => Some(Ok(req)),
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

