pub mod gateway;
pub mod resource_manager;
pub mod message;
pub mod rpc;

use std::sync::MutexGuard;
use std::sync::mpsc::Sender;
use uuid::Uuid;
use crate::request::{LabeledInvoke, Response};
use resource_manager::ResourceManager;


#[derive(Debug)]
pub enum Error {
    Rpc(prost::DecodeError),
    StreamConnect(std::io::Error),
    StreamRead(std::io::Error),
    StreamWrite(std::io::Error),
}

fn schedule(
    invoke: LabeledInvoke,
    manager: &mut MutexGuard<ResourceManager>,
    uuid: Uuid,
) -> Result<(), Error> {
    let gate = &invoke.gate.image;

    let mut stream = manager
        .find_idle(gate)
        .map(|w| w.stream)
        .unwrap_or_else(|| {
            panic!("no idle worker found")
        });

    // forward http request
    use message::response::Kind as ResKind;
    let invoke = invoke.to_vec();
    let res = message::Response {
        kind: Some(ResKind::ProcessJob(message::ProcessJob {
            id: uuid.to_string(), invoke,
        })),
    };
    let _ = message::write(&mut stream, res)?;

    // response are received as an message
    Ok(())
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
    invoke: LabeledInvoke, manager: gateway::Manager, tx: Sender<Response>
) -> Result<(), Error> {
    let mut manager = manager.lock().unwrap();
    let uuid = Uuid::new_v4();
    manager.wait_list.insert(uuid.clone(), tx);
    schedule(invoke, &mut manager, uuid)
}
