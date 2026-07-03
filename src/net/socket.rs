//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ffi::{c_int, c_void};
use core::mem::size_of;
use core::net::{Ipv4Addr, SocketAddrV4};

use crate::{ErrorKind, OrtResult, Read, Write, ort_error, syscall, utils};

pub struct TcpSocket {
    fd: i32,
}

impl TcpSocket {
    pub fn new() -> OrtResult<Self> {
        let fd = syscall::socket(
            syscall::AF_INET,
            syscall::SOCK_STREAM | syscall::SOCK_CLOEXEC,
            0,
        );
        if fd == -1 {
            return Err(ort_error(ErrorKind::SocketCreateFailed, ""));
        }
        set_tcp_fastopen(fd);
        Ok(TcpSocket { fd })
    }

    pub fn connect(&self, addr: &SocketAddrV4, timeout_ms: c_int) -> OrtResult<()> {
        let c_addr = socket_addr_v4_to_c(addr);
        let len = size_of::<syscall::sockaddr_in>() as syscall::socklen_t;

        // Get the current flags
        let flags = syscall::fcntl(self.fd, syscall::F_GETFL, 0);
        if flags < 0 {
            return Err(ort_error(ErrorKind::SocketConnectFailed, ""));
        }
        // Set socket to non blocking by adding to current flags
        syscall::fcntl(self.fd, syscall::F_SETFL, flags | syscall::O_NONBLOCK);

        let res = syscall::connect(
            self.fd,
            &c_addr as *const _ as *const syscall::sockaddr,
            len,
        );
        // connect failed before we even started, not sure when this can happen
        if res < 0 && res != syscall::EINPROGRESS {
            syscall::fcntl(self.fd, syscall::F_SETFL, flags);
            return Err(ort_error(
                ErrorKind::SocketConnectFailed,
                "non-blocking connect failed",
            ));
        }
        // Normal case, we are EINPROGRESS
        if res < 0 {
            let mut err: c_int = 0;
            let mut err_len = size_of::<c_int>() as syscall::socklen_t;
            // poll syscall has built-in timeout
            // getsockopt after poll succeeds to check for socket error
            if syscall::poll_write(self.fd, timeout_ms) <= 0
                || syscall::getsockopt(
                    self.fd,
                    syscall::SOL_SOCKET,
                    syscall::SO_ERROR,
                    &mut err as *mut _ as *mut c_void,
                    &mut err_len,
                ) < 0
                || err != 0
            {
                syscall::fcntl(self.fd, syscall::F_SETFL, flags);
                return Err(ort_error(ErrorKind::SocketConnectFailed, "timed out"));
            }
        }
        // Success. Set socket back to blocking mode by restoring flags
        syscall::fcntl(self.fd, syscall::F_SETFL, flags);
        Ok(())
    }
}

impl super::AsFd for TcpSocket {
    fn as_fd(&self) -> i32 {
        self.fd
    }
}

impl Read for TcpSocket {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        let bytes_read = syscall::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len());
        if bytes_read < 0 {
            if bytes_read == syscall::EAGAIN {
                return Err(ort_error(ErrorKind::WouldBlock, ""));
            }
            // see /usr/include/asm-generic/errno.h to translate the codes
            let err_code = utils::num_to_string(-bytes_read);
            utils::print_string(c"socket read err: ", &err_code);
            Err(ort_error(ErrorKind::SocketReadFailed, "syscall read error"))
        } else {
            Ok(bytes_read as usize)
        }
    }
}

impl Write for TcpSocket {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written = syscall::write(self.fd, buf.as_ptr() as *const c_void, buf.len());
        if bytes_written < 0 {
            // see /usr/include/asm-generic/errno.h to translate the codes
            let err_code = utils::num_to_string(-bytes_written);
            utils::print_string(c"socket write err: ", &err_code);
            Err(ort_error(
                ErrorKind::SocketWriteFailed,
                "syscall write error",
            ))
        } else {
            Ok(bytes_written as usize)
        }
    }

    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}

/// Must be called before 'connect'.
fn set_tcp_fastopen(fd: i32) {
    let optval: c_int = 1; // Enable
    syscall::setsockopt(
        fd,
        syscall::IPPROTO_TCP,
        syscall::TCP_FASTOPEN_CONNECT,
        &optval as *const _ as *const core::ffi::c_void,
        size_of::<i32>() as u32,
    );
}

fn socket_addr_v4_to_c(addr: &SocketAddrV4) -> syscall::sockaddr_in {
    syscall::sockaddr_in {
        sin_family: syscall::AF_INET as syscall::sa_family_t,
        sin_port: addr.port().to_be(),
        sin_addr: ip_v4_addr_to_c(addr.ip()),
        ..unsafe { core::mem::zeroed() }
    }
}
fn ip_v4_addr_to_c(addr: &Ipv4Addr) -> syscall::in_addr {
    // `s_addr` is stored as BE on all machines and the array is in BE order.
    // So the native endian conversion method is used so that it's never swapped.
    syscall::in_addr {
        s_addr: u32::from_ne_bytes(addr.octets()),
    }
}
