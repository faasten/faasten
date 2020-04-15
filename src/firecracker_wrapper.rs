/// Wrapper for Firecracker vmm and vm
use nix::{Error};
use std::path::{PathBuf};
use std::sync::{Arc, RwLock, mpsc::Sender, mpsc::channel};
use std::rc::Rc;
use std::fs::File;
use std::thread::JoinHandle;
use std::os::unix::io::FromRawFd;
use std::io::{self, Read, Write};

use futures::Future;
use futures::sync::oneshot;
use vmm::{VmmAction, VmmActionError, VmmData};
use vmm::vmm_config::instance_info::{InstanceInfo, InstanceState};
use vmm::vmm_config::boot_source::BootSourceConfig;
use vmm::vmm_config::drive::BlockDeviceConfig;
use vmm::vmm_config::vsock::VsockDeviceConfig;
use vmm::vmm_config::machine_config::VmConfig;
use vmm::vmm_config::logger::{LoggerConfig, LoggerLevel};
use sys_util::EventFd;

pub struct VmmWrapper {
    vmm_thread_handle: JoinHandle<()>,
    vmm_action_sender: Sender<Box<VmmAction>>,
    event_fd: Rc<EventFd>,
    shared_info: Arc<RwLock<InstanceInfo>>,
}

pub struct VmChannel {
    request_sender: File,
    response_receiver: File,
    status_receiver: File,
}

pub enum VmmError {
    PipeCreate(nix::Error),
    EventFd(io::Error),
}

impl VmmWrapper {
    pub fn new(id: String, load_dir:Option<PathBuf>, dump_dir:Option<PathBuf>)
        -> Result<(VmmWrapper, VmChannel), VmmError> {

        // unix pipes for communicating with the vm
        let (request_receiver, request_sender) = nix::unistd::pipe().map_err(|e| VmmError::PipeCreate(e))?;
        let (response_receiver, response_sender) = nix::unistd::pipe().map_err(|e| VmmError::PipeCreate(e))?;
        let (status_receiver, status_sender) = nix::unistd::pipe().map_err(|e| VmmError::PipeCreate(e))?;
        // mpsc channel for Box<VmmAction> with the vmm thread
        let (vmm_action_sender, vmm_action_receiver) = channel();

        let shared_info = Arc::new(RwLock::new(InstanceInfo {
                            state: InstanceState::Uninitialized,
                            id: id.clone(),
                            vmm_version: "0.1".to_string(),
                            load_dir: load_dir,
                            dump_dir: dump_dir,
                            }));

        let event_fd = EventFd::new().map_err(|e| VmmError::EventFd(e))?;
        let event_fd = Rc::new(event_fd);
        let event_fd_clone = event_fd.try_clone().map_err(|e| VmmError::EventFd(e))?;

        let thread_handle =
            vmm::start_vmm_thread(shared_info.clone(),
                                  event_fd_clone,
                                  vmm_action_receiver,
                                  0, //seccomp::SECCOMP_LEVEL_NONE,
                                  Some(unsafe { File::from_raw_fd(response_sender) }),
                                  Some(unsafe { File::from_raw_fd(request_receiver) }),
                                  Some(unsafe { File::from_raw_fd(status_sender) }),
                                  id.parse::<u32>().unwrap() //TODO: remove notifier ID completely. Just pass in a dummy value for now
                                  );
        
        let vmm_wrapper = VmmWrapper {
            vmm_thread_handle: thread_handle,
            vmm_action_sender: vmm_action_sender,
            event_fd: event_fd,
            shared_info: shared_info,
        };
        
        let vm_wrapper = VmChannel {
            request_sender: unsafe { File::from_raw_fd(request_sender) },
            response_receiver: unsafe { File::from_raw_fd(response_receiver) },
            status_receiver: unsafe { File::from_raw_fd(status_receiver) },
        };

        return Ok((vmm_wrapper, vm_wrapper));
    }

}
