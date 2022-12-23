pub mod gateway;
pub mod resource_manager;
pub mod message;
pub mod rpc;

use std::error::Error;
use crate::request::LabeledInvoke;
// use crate::resource_manager::ResourceManager
    // as LocalResourceManger;
// use self::resource_manager::ResourceManager;
use message::{
    request, response,
    Request, Response,
};
// use self::resource_manager::LocalResourceManagerInfo;

/// This method schedules a http request to a remote worker
pub fn schedule(
    invoke: LabeledInvoke, resman: self::gateway::Manager,
) -> Result<(), Box<dyn Error>> {
    let mut resman = resman.lock().unwrap();
    let function = &invoke.function;

    // TODO when no idle worker found
    let mut stream = resman
        .find_idle(function)
        .map(|w| w.stream)
        .unwrap_or_else(|| {
            panic!("no idle worker found")
        });

    // forward http request
    let buf = invoke.to_vec();
    let res = Response {
        kind: Some(response::Kind::Process(buf)),
    };
    let _ = message::write(&mut stream, res)?;

    // response are received as an message
    Ok(())
}

