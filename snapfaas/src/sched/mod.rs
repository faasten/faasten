pub mod gateway;
pub mod resource_manager;
pub mod message;
pub mod rpc;

use std::sync::MutexGuard;
use std::sync::mpsc::Sender;
use uuid::Uuid;
use resource_manager::ResourceManager;
use message::LabeledInvoke;

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

fn schedule(
    invoke: LabeledInvoke,
    manager: &mut MutexGuard<ResourceManager>,
    uuid: Uuid,
) -> Result<(), Error> {
    let gate = &invoke.gate.image;
    let task_sender = manager
        .find_idle(gate)
        .map(|w| w.sender)
        .unwrap_or_else(|| {
            panic!("no idle worker found")
        });
    task_sender.send(Task::Invoke(uuid, invoke))
        .map_err(|e| Error::TaskSend(e))
}

/// This method schedules an async invoke to a remote worker
pub fn schedule_async(
    invoke: LabeledInvoke, manager: gateway::Manager,
) -> Result<(), Error> {
    let mut manager = manager.lock().unwrap();
    let uuid = Uuid::nil();
    schedule(invoke, &mut manager, uuid)
}

/// This method schedules a sync invoke to a remote worker
pub fn schedule_sync(
    invoke: LabeledInvoke, manager: gateway::Manager, tx: Sender<String>
) -> Result<(), Error> {
    let mut manager = manager.lock().unwrap();
    let uuid = Uuid::new_v4();
    manager.wait_list.insert(uuid.clone(), tx);
    schedule(invoke, &mut manager, uuid)
}
