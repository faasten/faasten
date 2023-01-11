include!(concat!(env!("OUT_DIR"), "/snapfaas.sched.messages.rs"));

use std::io::{Read, Write};
use std::net::TcpStream;
use prost::Message;

use super::Error;

fn recv_from(stream: &mut TcpStream) -> Result<Vec<u8>, Error> {
    let mut lenbuf = [0; 4];
    stream.read_exact(&mut lenbuf)
          .map_err(|e| Error::StreamRead(e))?;
    let size = u32::from_be_bytes(lenbuf);
    let mut buf = vec![0u8; size as usize];
    stream.read_exact(&mut buf)
          .map_err(|e| Error::StreamRead(e))?;
    Ok(buf)
}

fn send_to(stream: &mut TcpStream, msg: Vec<u8>) -> Result<(), Error> {
    stream.write_all(&(msg.len() as u32).to_be_bytes())
          .map_err(|e| Error::StreamWrite(e))?;
    stream.write_all(msg.as_ref())
          .map_err(|e| Error::StreamWrite(e))?;
    Ok(())
}

/// Wrapper function that sends a message
pub fn write<T: Message>(stream: &mut TcpStream, msg: T) -> Result<(), Error> {
    let buf = msg.encode_to_vec();
    send_to(stream, buf)
}

/// Wrapper function that reads a request
pub fn read_request(stream: &mut TcpStream) -> Result<Request, Error> {
    let buf = recv_from(stream)?;
    Request::decode(&buf[..]).map_err(|e| Error::Rpc(e))
}

/// Wrapper function that reads a response
pub fn read_response(stream: &mut TcpStream) -> Result<Response, Error> {
    let buf = recv_from(stream)?;
    Response::decode(&buf[..]).map_err(|e| Error::Rpc(e))
}

/// Function that reads bytes from a stream
pub fn read_u8(stream: &mut TcpStream) -> Result<Vec<u8>, Error> {
    let mut lenbuf = [0; 4];
    stream.read_exact(&mut lenbuf)
          .map_err(|e| Error::StreamRead(e))?;
    let size = u32::from_be_bytes(lenbuf);
    if size > 0 {
        let mut buf = vec![0u8; size as usize];
        stream.read_exact(&mut buf)
              .map_err(|e| Error::StreamRead(e))?;
        Ok(buf)
    } else {
        Err(Error::Other("Empty Payload".to_string()))
    }
}

/// Function that writes bytes to a stream
pub fn write_u8(stream: &mut TcpStream, buf: Vec<u8>) -> Result<(), Error> {
    send_to(stream, buf)
}

/// Wrapper function that parses bytes to labeled invoke message
pub fn parse_u8_labeled_invoke(buf: Vec<u8>) -> Result<LabeledInvoke, Error> {
    LabeledInvoke::decode(&buf[..]).map_err(|e| Error::Rpc(e))
}
