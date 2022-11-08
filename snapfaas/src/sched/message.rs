include!(concat!(env!("OUT_DIR"), "/snapfaas.sched.messages.rs"));

use std::io::{Read, Write};
use std::net::TcpStream;
use std::error::Error;
use prost::Message;

fn recv_from(
    stream: &mut TcpStream,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut lenbuf = [0; 4];
    stream.read_exact(&mut lenbuf)?;
    let size = u32::from_be_bytes(lenbuf);
    let mut buf = vec![0u8; size as usize];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

fn send_to(
    stream: &mut TcpStream,
    msg: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    stream.write_all(&(msg.len() as u32).to_be_bytes())?;
    stream.write_all(msg.as_ref())
        .map_err(|e| -> Box<dyn Error> {
            Box::new(e) as _
        })?;
    Ok(())
}

/// Wrapper function that sends a message
pub fn write<T: Message>(
    stream: &mut TcpStream,
    msg: T,
) -> Result<(), Box<dyn Error>> {
    let buf = msg.encode_to_vec();
    send_to(stream, buf)
}

/// Wrapper function that reads a request
pub fn read_request(
    stream: &mut TcpStream
) -> Result<Request, Box<dyn Error>> {
    let buf = recv_from(stream)?;
    let req = Request::decode(&buf[..])?;
    Ok(req)
}

/// Wrapper function that reads a response
pub fn read_response(
    stream: &mut TcpStream
) -> Result<Response, Box<dyn Error>> {
    let buf = recv_from(stream)?;
    let req = Response::decode(&buf[..])?;
    Ok(req)
}
