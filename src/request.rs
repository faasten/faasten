use std::io::{Error, ErrorKind, Write, Read};
use serde::{Deserialize, Serialize};
use serde_json::Value;
                
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub user_id: u64,
    pub payload: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub time: u64,
    pub user_id: u64,
    pub function: String,
    pub payload: Value,
}

pub fn parse_json(json: &str) -> Result<Request, serde_json::Error> {
    serde_json::from_str(json)
}

/// Given a [u8], parse it to a Requst value
pub fn parse_u8(buf: Vec<u8>) -> Result<Request, serde_json::Error> {
    let json = String::from_utf8(buf).unwrap_or("not json string".to_string());
    serde_json::from_str(&json)
}

pub fn write_u8_vm(buf: &[u8], channel: &mut dyn Write) ->  std::io::Result<()> {
    let size = (buf.len() as u32).to_be_bytes();
    channel.write_all(&size)?;
    channel.write_all(buf)?;
    return Ok(());
}

pub fn read_u8_vm(channel: &mut dyn Read) -> std::io::Result<Vec<u8>>{
    let mut buf = [0;4];
    channel.read_exact(&mut buf)?;
    let size = u32::from_be_bytes(buf);

    if size > 0 {
        let mut buf = vec![0u8; size as usize];
        channel.read_exact(&mut buf)?;
        return Ok(buf);
    }

    return Err(Error::new(ErrorKind::Other, "Empty payload"));
}
/// Write a [u8] array to channel.
/// Channel is anything that implements the std::io::Read trait. In practice,
/// this includes TcpStream and stdin pipe for Firerunner processes.
/// `write_u8` will first write 4 bytes ([u8;4]) into the channel. These 4 bytes
/// encodes the size of the buf in big endian. It then writes the buf into the
/// channel. 
/// On success, `write_u8` returns Ok(()).
/// If either one of the `write_all()` calls fails, `write_u8` returns
/// the corresponding std::io::Error.
pub fn write_u8(buf: &[u8], channel: &mut std::net::TcpStream) ->  std::io::Result<()> {
    let size = buf.len().to_be_bytes();
    let mut data = buf.to_vec();
    for i in (0..size.len()).rev() {
        data.insert(0, size[i]);
    }
    //channel.write_all(&size.to_be_bytes())?;
    channel.write_all(&data)?;
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
        //channel.set_nonblocking(false).expect("cannot set stream to blocking");
        let mut buf = vec![0; size as usize];
        channel.read_exact(&mut buf)?;
        //channel.set_nonblocking(true).expect("cannot set stream to non-blocking");
        return Ok(buf);
    }

    return Err(Error::new(ErrorKind::Other, "Empty payload"));
}

impl Request {

    /// return a Request value as a JSON string
    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        return serde_json::to_string(&self);
    }

    /// return payload as a JSON string
    pub fn payload_as_string(&self) -> String {
        return self.payload.to_string();
    }

}
