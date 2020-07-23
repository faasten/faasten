use std::os::unix::io::RawFd;
use std::io::{self, Read, Write};

pub const VMADDR_CID_ANY: u32 = libc::VMADDR_CID_ANY;
pub const VMADDR_CID_HOST: u32 = libc::VMADDR_CID_HOST;
/// port that vsock listener listens on
pub const VSOCKPORT: u32 = 1234;

#[derive(Debug)]
pub struct VsockAddr {
    pub port: u32,
    pub cid: u32,
}

#[derive(Debug)]
pub struct VsockListener {
    fd: RawFd
}

impl VsockListener {
    pub fn bind(address: u32, port: u32) -> io::Result<VsockListener> {
        let sock = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
        if sock < 0 {
            return Err(nix::errno::Errno::last().into());
        }

        let sockaddr = libc::sockaddr_vm {
            svm_family: libc::AF_VSOCK as u16,
            svm_reserved1: 0,
            svm_port: port,
            svm_cid: address,
            svm_zero: [0, 0, 0, 0],
        };
        unsafe {
            if libc::bind(sock, &sockaddr as *const _ as *const _, std::mem::size_of::<libc::sockaddr_vm>() as u32) < 0 {
                return Err(nix::errno::Errno::last().into());
            }
            if libc::listen(sock, 128) < 0 {
                return Err(nix::errno::Errno::last().into());
            }
        }
        Ok(VsockListener {
            fd: sock,
        })
    }

    pub fn accept(&mut self) -> io::Result<(VsockStream, VsockAddr)> {
        let mut client_sockaddr = unsafe { std::mem::MaybeUninit::<libc::sockaddr_vm>::uninit().assume_init() };
        let mut i = std::mem::size_of::<libc::sockaddr_vm>() as u32;
        let connection = unsafe {
            let sock = libc::accept(self.fd, &mut client_sockaddr as *mut _ as *mut libc::sockaddr, &mut i as *mut _);
            if sock < 0 {
                return Err(nix::errno::Errno::last().into());
            } else {
                sock
            }
        };
        let sockaddr = VsockAddr {
            cid: client_sockaddr.svm_cid,
            port: client_sockaddr.svm_port,
        };
        Ok((VsockStream(connection), sockaddr))
    }

    pub fn closer(&mut self) -> VsockCloser {
        VsockCloser(self.fd)
    }
}

#[derive(Debug)]
pub struct VsockCloser(RawFd);

impl VsockCloser {
    pub fn close(&self) -> io::Result<usize> {
        let ret = unsafe { libc::close(self.0) };
        if ret < 0 {
            return Err(nix::errno::Errno::last().into());
        }
        Ok(ret as usize)
    }
}

#[derive(Debug)]
pub struct VsockStream(RawFd);

impl VsockStream {
    pub fn close(&self) -> io::Result<usize> {
        VsockCloser(self.0).close()
    }
}

impl Read for VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = unsafe { libc::recv(self.0,
                       buf.as_mut_ptr() as *mut libc::c_void,
                       buf.len(),
                       0)
        };
        if ret < 0 {
            return Err(nix::errno::Errno::last().into());
        }
        Ok(ret as usize)
    }
}

impl Write for VsockStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let ret = unsafe { libc::send(self.0,
                       buf.as_ptr() as *const libc::c_void,
                       buf.len(),
                       0)
        };
        if ret < 0 {
            return Err(nix::errno::Errno::last().into());
        }
        Ok(ret as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
