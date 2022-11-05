include!(concat!(env!("OUT_DIR"), "/snapfaas.sched.messages.rs"));

use std::io::{Read, Write};
use std::net::TcpStream;
use std::error::Error;
// use prost::Message;

pub fn recv_from(
    stream: &mut TcpStream,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut lenbuf = [0; 4];
    stream.read_exact(&mut lenbuf)?;
    let size = u32::from_be_bytes(lenbuf);
    let mut buf = vec![0u8; size as usize];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn send_to(
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
