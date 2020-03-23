use std::io::{Error, ErrorKind, Write, Read};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub function: String,
    pub payload: Value,
    pub time: u64,
}

pub fn parse_json(json: &str) -> Result<Request, serde_json::Error> {
    serde_json::from_str(json)
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
pub fn write_u8<T: Write>(buf: &[u8], channel: &mut T) ->  std::io::Result<()> {
    let size = buf.len();
    channel.write_all(&size.to_be_bytes())?;
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
    let mut buf = [0;4];
    channel.read_exact(&mut buf)?;
    let size = u32::from_be_bytes(buf);

    if size == 0 {
        return read_u8(channel);
    }

    if size > 0 {
        channel.set_nonblocking(false).expect("cannot set stream to blocking");
        let mut buf = vec![0; size as usize];
        channel.read_exact(&mut buf)?;
        channel.set_nonblocking(true).expect("cannot set stream to non-blocking");
        return Ok(buf);
    }

    return Err(Error::new(ErrorKind::Other, "negative-sized payload"));
}

impl Request {

    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        return serde_json::to_string(&self.payload);
    }

}
