//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ffi::{c_int, c_void};
use core::mem::size_of;
use core::net::{Ipv4Addr, SocketAddrV4};

use crate::{OrtResult, Read, Write, libc, ort_err};

pub struct TcpSocket {
    fd: i32,
}

impl TcpSocket {
    pub fn new() -> OrtResult<Self> {
        let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
        if fd == -1 {
            return ort_err("libc::socket failed");
        }
        set_tcp_fastopen(fd);
        Ok(TcpSocket { fd })
    }

    pub fn connect(&self, addr: &SocketAddrV4) -> OrtResult<()> {
        let c_addr = socket_addr_v4_to_c(addr);
        let len = size_of::<libc::sockaddr_in>() as libc::socklen_t;
        let res =
            unsafe { libc::connect(self.fd, &c_addr as *const _ as *const libc::sockaddr, len) };
        if res == -1 {
            return ort_err("connect failed");
        }
        Ok(())
    }
}

impl Read for TcpSocket {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        let bytes_read = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
        if bytes_read < 0 {
            ort_err("syscall read error")
        } else {
            Ok(bytes_read as usize)
        }
    }
}

impl Write for TcpSocket {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written =
            unsafe { libc::write(self.fd, buf.as_ptr() as *const c_void, buf.len()) };
        if bytes_written < 0 {
            ort_err("syscall write error")
        } else {
            Ok(bytes_written as usize)
        }
    }

    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}

fn set_tcp_fastopen(fd: i32) {
    let optval: c_int = 1; // Enable
    unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_FASTOPEN,
            &optval as *const _ as *const core::ffi::c_void,
            size_of::<i32>() as u32,
        );
    }
}

fn socket_addr_v4_to_c(addr: &SocketAddrV4) -> libc::sockaddr_in {
    libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: addr.port().to_be(),
        sin_addr: ip_v4_addr_to_c(addr.ip()),
        ..unsafe { core::mem::zeroed() }
    }
}
fn ip_v4_addr_to_c(addr: &Ipv4Addr) -> libc::in_addr {
    // `s_addr` is stored as BE on all machines and the array is in BE order.
    // So the native endian conversion method is used so that it's never swapped.
    libc::in_addr {
        s_addr: u32::from_ne_bytes(addr.octets()),
    }
}
