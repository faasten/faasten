/// Wrapper for Firecracker vmm and vm
use std::sync::{Arc, RwLock, mpsc, mpsc::Sender, mpsc::channel};
use std::rc::Rc;
use std::thread::JoinHandle;
use std::io;

use futures::Future;
use futures::sync::oneshot;
use vmm::{VmmAction, VmmActionError, VmmData, VmmRequestOutcome};
use vmm::vmm_config::instance_info::{InstanceInfo, InstanceState};
use vmm::vmm_config::boot_source::BootSourceConfig;
use vmm::vmm_config::drive::BlockDeviceConfig;
use vmm::vmm_config::net::NetworkInterfaceConfig;
use vmm::vmm_config::vsock::VsockDeviceConfig;
use vmm::vmm_config::machine_config::VmConfig;
use vmm::SnapFaaSConfig;
use sys_util::EventFd;

pub struct VmmWrapper {
    vmm_thread_handle: JoinHandle<()>,
    vmm_action_sender: Sender<Box<VmmAction>>,
    event_fd: Rc<EventFd>,
}

//#[derive(Debug)]
//pub struct VmChannel {
//    request_sender: File,
//    response_receiver: File,
//    status_receiver: File,
//}

#[derive(Debug)]
pub enum VmmError {
    PipeCreate(nix::Error),
    EventFd(io::Error),
    ActionError(VmmActionError),
    ActionSender(mpsc::SendError<Box<VmmAction>>),
    SyncChannel(oneshot::Canceled),
}

impl VmmWrapper {
    pub fn new(id: String, config: SnapFaaSConfig) -> Result<VmmWrapper, VmmError> {

        // unix pipes for communicating with the vm
        //let (request_receiver, request_sender) = nix::unistd::pipe().map_err(|e| VmmError::PipeCreate(e))?;
        //let (response_receiver, response_sender) = nix::unistd::pipe().map_err(|e| VmmError::PipeCreate(e))?;
        //let (status_receiver, status_sender) = nix::unistd::pipe().map_err(|e| VmmError::PipeCreate(e))?;
        // mpsc channel for Box<VmmAction> with the vmm thread
        let (vmm_action_sender, vmm_action_receiver) = channel();

        let shared_info = Arc::new(RwLock::new(InstanceInfo {
                            state: InstanceState::Uninitialized,
                            id: id.clone(),
                            vmm_version: "0.1".to_string(),
        }));

        let event_fd = EventFd::new().map_err(|e| VmmError::EventFd(e))?;
        let event_fd = Rc::new(event_fd);
        let event_fd_clone = event_fd.try_clone().map_err(|e| VmmError::EventFd(e))?;

        let thread_handle =
            vmm::start_vmm_thread(shared_info.clone(),
                                  event_fd_clone,
                                  vmm_action_receiver,
                                  0, //seccomp::SECCOMP_LEVEL_NONE,
                                  config,
                                  );
        
        let vmm_wrapper = VmmWrapper {
            vmm_thread_handle: thread_handle,
            vmm_action_sender: vmm_action_sender,
            event_fd: event_fd,
        };
        
        //let vm_wrapper = VmChannel {
        //    request_sender: unsafe { File::from_raw_fd(request_sender) },
        //    response_receiver: unsafe { File::from_raw_fd(response_receiver) },
        //    status_receiver: unsafe { File::from_raw_fd(status_receiver) },
        //};

        return Ok(vmm_wrapper);
    }

    pub fn send_vmm_action(&mut self, action: VmmAction) -> Result<(), VmmError> {
        self.vmm_action_sender.send(Box::new(action)).map_err(|e| VmmError::ActionSender(e))?;
        self.event_fd.write(1).map_err(|e| VmmError::EventFd(e))?;
        return Ok(());
    }

    pub fn recv_vmm_action_ret(&self, receiver: oneshot::Receiver<VmmRequestOutcome>)
        -> Result<VmmData, VmmError> {
        let ret = receiver.wait().map_err(|e| VmmError::SyncChannel(e))?;
        return ret.map_err(|e| VmmError::ActionError(e));
    }

    pub fn request_vmm_action(&mut self,
                              action: VmmAction,
                              ret_receiver: oneshot::Receiver<VmmRequestOutcome>)
        -> Result<VmmData, VmmError> {
        self.send_vmm_action(action)?;
        self.recv_vmm_action_ret(ret_receiver)
    }

    pub fn set_configuration(&mut self, machine_config: VmConfig) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::SetVmConfiguration(machine_config, sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }

    pub fn get_configuration(&mut self) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::GetVmConfiguration(sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }

    pub fn set_boot_source(&mut self, config: BootSourceConfig) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::ConfigureBootSource(config, sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }

    pub fn insert_block_device(&mut self, config: BlockDeviceConfig) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::InsertBlockDevice(config, sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }

    pub fn insert_network_device(&mut self, config: NetworkInterfaceConfig) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::InsertNetworkDevice(config, sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }

    pub fn add_vsock(&mut self, config: VsockDeviceConfig) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::InsertVsockDevice(config, sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }


    pub fn start_instance(&mut self) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::StartMicroVm(sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }

    pub fn shutdown_instance(&mut self) -> Result<VmmData, VmmError> {
        let (sync_sender, sync_receiver) = oneshot::channel();
        let action = VmmAction::SendCtrlAltDel(sync_sender);
        self.request_vmm_action(action, sync_receiver)
    }

    pub fn join_vmm(self) {
        self.vmm_thread_handle.join().expect("Couldn't join on the VMM thread");
    }

}

//impl VmChannel {
//    /// TODO: Add a timeout to this read
//    /// Read ready signal from the vm
//    pub fn recv_status(&mut self) -> Result<[u8;4], std::io::Error> {
//        let data = &mut[0u8; 4];
//        self.status_receiver.read_exact(data)?;
//        return Ok(*data);
//    }
//
//    /// Send a request to vm
//    pub fn send_request_u8(&mut self, req: &[u8]) -> Result<(), std::io::Error> {
//        self.request_sender.write_all(req)
//    }
//
//    pub fn recv_response_string(&mut self) -> Result<String, std::io::Error> {
//        let mut lens = [0; 4];
//        self.response_receiver.read_exact(&mut lens)?;
//        let len = u32::from_be_bytes(lens);
//
//        let mut response = vec![0; len as usize];
//        self.response_receiver.read_exact(response.as_mut_slice())?;
//
//        return String::from_utf8(response)
//                 .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData,"Not UTF8"));
//    }
//}
