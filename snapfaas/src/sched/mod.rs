pub mod message;
pub mod resource_manager;
pub mod rpc;
pub mod rpc_server;

use log::error;
use message::LabeledInvoke;
use std::{
    net::{SocketAddr, TcpStream},
    str::FromStr,
    sync::{mpsc::Sender, Arc, Condvar, Mutex},
};
use uuid::Uuid;

use self::resource_manager::ResourceManager;

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
    Terminate,
}

/// simple fifo
pub fn schedule(
    queue_rx: crossbeam::channel::Receiver<Task>,
    manager: Arc<Mutex<ResourceManager>>,
    cvar: Arc<Condvar>,
) {
    while let Ok(task) = queue_rx.recv() {
        let f = match &task {
            Task::Invoke(_, li) => li.function.as_ref().unwrap().clone().into(),
            _ => panic!("Unexpected task {:?}", task),
        };
        use message::response::Kind as ResKind;
        // we might be given broken stream, so if message::write fail
        // we loop to try again
        loop {
            let mut maybe_worker: Option<resource_manager::Worker>;
            {
                // wait till there is an idle worker.
                let mut manager = manager.lock().unwrap();
                loop {
                    maybe_worker = manager.find_idle(&f);
                    if maybe_worker.is_none() {
                        manager = cvar.wait(manager).unwrap();
                    } else {
                        break;
                    }
                }
            }
            let mut worker = maybe_worker.unwrap();
            match &task {
                Task::Invoke(uuid, labeled_invoke) => {
                    let res = message::Response {
                        kind: Some(ResKind::ProcessTask(message::ProcessTask {
                            task_id: uuid.to_string(),
                            labeled_invoke: Some(labeled_invoke.clone()),
                        })),
                    };
                    if let Err(e) = message::write(&mut worker.conn, &res) {
                        error!("{:?}. try again.", e);
                    } else {
                        break;
                    }
                }
                _ => panic!("Unexpected task {:?}", task),
            }
        } // retry loop upon message::write failure
    }
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
