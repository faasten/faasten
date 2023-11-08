include!(concat!(env!("OUT_DIR"), "/snapfaas.sched.messages.rs"));

use prost::Message;
use rouille;
use std::io::{Read, Write};
use std::net::TcpStream;

use super::Error;

// respond 500 if fail to start the execution of the requested gate; otherwise, act as a passthrough, i.e., respond whatever the execution responds.
impl From<TaskReturn> for rouille::Response {
    fn from(tr: TaskReturn) -> rouille::Response {
        use rouille::Response;
        let mut resp = match ReturnCode::from_i32(tr.code) {
            Some(ReturnCode::QueueFull) => Response::json(&serde_json::json!({
                "error": "queue full"
            }))
            .with_status_code(500),
            Some(ReturnCode::LaunchFailed) => Response::json(&serde_json::json!({
                "error": "failed to launch the VM"
            }))
            .with_status_code(500),
            Some(ReturnCode::GateNotExist) => Response::json(&serde_json::json!({
                "error": "gate does not exist"
            }))
            .with_status_code(500),
            Some(ReturnCode::ResourceExhausted) => Response::json(&serde_json::json!({
                "error": "resource exhausted"
            }))
            .with_status_code(500),
            Some(ReturnCode::ProcessRequestFailed) => Response::json(&serde_json::json!({
                "error": "failed to process request"
            }))
            .with_status_code(500),
            Some(ReturnCode::Success) => Response::from_data(
                "application/octet-stream",
                tr.payload.as_ref().unwrap().body(),
            ),
            None => Response::json(&serde_json::json!({
                "error": "unknown return code"
            }))
            .with_status_code(500),
        };
        // act as a passthrough
        if resp.is_success() {
            resp = resp.with_status_code(tr.payload.unwrap().status_code as u16);
        }
        resp
    }
}

fn _read_u8(stream: &mut TcpStream, allow_empty: bool) -> Result<Vec<u8>, Error> {
    let mut lenbuf = [0; 8];
    stream
        .read_exact(&mut lenbuf)
        .map_err(|e| Error::StreamRead(e))?;
    let size = u64::from_be_bytes(lenbuf);
    if allow_empty || size > 0 {
        let mut buf = vec![0u8; size as usize];
        stream
            .read_exact(&mut buf)
            .map_err(|e| Error::StreamRead(e))?;
        Ok(buf)
    } else {
        Err(Error::Other("Empty Payload".to_string()))
    }
}

/// Function that reads bytes from a stream
pub fn read_u8(stream: &mut TcpStream) -> Result<Vec<u8>, Error> {
    _read_u8(stream, false)
}

/// Function that writes bytes to a stream
pub fn write_u8(stream: &mut TcpStream, msg: &[u8]) -> Result<(), Error> {
    let size = (msg.len() as u64).to_be_bytes();
    stream.write_all(&size).map_err(|e| Error::StreamWrite(e))?;
    stream.write_all(msg).map_err(|e| Error::StreamWrite(e))?;
    Ok(())
}

/// Wrapper function that sends a message
pub fn write<T: Message>(stream: &mut TcpStream, msg: &T) -> Result<(), Error> {
    let buf = msg.encode_to_vec();
    write_u8(stream, &buf)
}

/// Wrapper function that reads a message
pub fn read<T: Message + Default>(stream: &mut TcpStream) -> Result<T, Error> {
    let buf = _read_u8(stream, true)?;
    T::decode(&buf[..]).map_err(Error::Rpc)
}

/// Wrapper function that reads a request
pub fn read_request(stream: &mut TcpStream) -> Result<Request, Error> {
    let buf = _read_u8(stream, true)?;
    Request::decode(&buf[..]).map_err(|e| Error::Rpc(e))
}

/// Wrapper function that reads a response
pub fn read_response(stream: &mut TcpStream) -> Result<Response, Error> {
    let buf = _read_u8(stream, true)?;
    Response::decode(&buf[..]).map_err(|e| Error::Rpc(e))
}
