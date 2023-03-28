pub mod message;
pub mod resource_manager;
pub mod rpc;
pub mod rpc_server;

use message::{LabeledInvoke, UnlabeledInvoke};
use std::{
    net::{SocketAddr, TcpStream},
    str::FromStr,
    sync::mpsc::Sender,
};
use uuid::Uuid;

pub type RequestInfo = (message::LabeledInvoke, Sender<String>);

#[derive(Debug)]
pub enum Error {
    Rpc(prost::DecodeError),
    TaskSend(std::sync::mpsc::SendError<Task>),
    StreamConnect(std::io::Error),
    StreamRead(std::io::Error),
    StreamWrite(std::io::Error),
    Other(String),
}

#[derive(Debug)]
pub enum Task {
    Invoke(Uuid, LabeledInvoke),
    InvokeInsecure(Uuid, UnlabeledInvoke),
    Terminate,
}

#[derive(Debug)]
pub struct Scheduler {
    addr: SocketAddr,
}

impl Scheduler {
    pub fn new(addr: &str) -> Self {
        Self {
            addr: SocketAddr::from_str(addr).unwrap(),
        }
    }
}

impl r2d2::ManageConnection for Scheduler {
    type Connection = TcpStream;
    type Error = std::io::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(TcpStream::connect(&self.addr)?)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        use std::io::{Error, ErrorKind};
        self::rpc::ping(conn).map_err(|e| Error::new(ErrorKind::Other, format!("{:?}", e)))?;
        Ok(())
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.take_error().ok().flatten().is_some()
    }
}
