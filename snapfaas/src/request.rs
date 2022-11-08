use std::io::{Error, ErrorKind, Write, Read};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use labeled::dclabel::DCLabel;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestStatus {
    Dropped,
    FunctionNotExist,
    ResourceExhausted,
    LaunchFailed,
    ProcessRequestFailed,
    SentToVM(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub status: RequestStatus,
}

impl Response {
    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(&self).unwrap()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Request {
    pub function: String,
    pub payload: Value,
    pub label: DCLabel,
    pub data_handles: HashMap<String, String>,
}

impl Default for Request {
    fn default() -> Self {
        Request {
            function: Default::default(),
            label: DCLabel::public(),
            payload: Default::default(),
            data_handles: Default::default(),
        }
    }
}

impl Request {
    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(&self).unwrap()
    }

    /// return payload as a JSON string
    pub fn payload_as_string(&self) -> String {
        return self.payload.to_string();
    }

}

/// Given a [u8], parse it to a Requst value
pub fn parse_u8_request(buf: Vec<u8>) -> Result<Request, serde_json::Error> {
    serde_json::from_slice(&buf)
}

/// Write a [u8] array to channel.
/// Channel is anything that implements the std::io::Read trait. In practice,
/// this includes TcpStream and stdin pipe for Firerunner processes.
/// `write_u8` will first write 4 bytes ([u8; 4]) into the channel. These 4 bytes
/// encodes the size of the buf in big endian. It then writes the buf into the
/// channel.
/// On success, `write_u8` returns Ok(()).
/// If either one of the `write_all()` calls fails, `write_u8` returns
/// the corresponding std::io::Error.
pub fn write_u8(buf: &[u8], channel: &mut std::net::TcpStream) ->  std::io::Result<()> {
    let size = buf.len().to_be_bytes();
    channel.write_all(&size)?;
    channel.write_all(buf)?;
    return Ok(());
}

/// Read a [u8] array from a channel.
/// Channel is anything that implements the std::io::Read trait. In practice,
/// this includes TcpStream and stdin pipe for Firerunner processes.
/// `read_u8` will first read 4 bytes from the channel. These 4 bytes encodes
/// the size of the payload in big endian. After deciding the size of the
/// payload, `read_u8` then read another [u8] buf of length `size` and return
/// Ok(buf).
/// If `size` is zero, `read_u8` will try again
/// If either one of the `read_exact()` calls fails, `read_u8` returns
/// the corresponding std::io::Error.
pub fn read_u8(channel: &mut std::net::TcpStream) -> std::io::Result<Vec<u8>>{
    let mut buf = [0;8];
    channel.read_exact(&mut buf)?;
    let size = u64::from_be_bytes(buf);

    if size > 0 {
        let mut buf = vec![0; size as usize];
        channel.read_exact(&mut buf)?;
        return Ok(buf);
    }

    Err(Error::new(ErrorKind::Other, "Empty payload"))
}
