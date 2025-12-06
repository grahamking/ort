//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![allow(non_camel_case_types)]

use core::ffi::{c_char, c_int, c_void};
use core::mem::size_of;
use core::net::{Ipv4Addr, SocketAddrV4};

use crate::{OrtResult, Read, Write, ort_err};

const SOCK_STREAM: c_int = 1;
const AF_INET: c_int = 2;

const O_CLOEXEC: c_int = 0x80000;
const SOCK_CLOEXEC: c_int = O_CLOEXEC;

const IPPROTO_TCP: i32 = 6;
const TCP_FASTOPEN: i32 = 23;

type size_t = usize;
type ssize_t = isize;

type socklen_t = u32;
type sa_family_t = u16;
type in_port_t = u16;
type in_addr_t = u32;

#[repr(C)]
struct in_addr {
    pub s_addr: in_addr_t,
}

#[repr(C)]
struct sockaddr_in {
    pub sin_family: sa_family_t,
    pub sin_port: in_port_t,
    pub sin_addr: in_addr,
    pub sin_zero: [u8; 8],
}

#[repr(C)]
struct sockaddr {
    pub sa_family: sa_family_t,
    pub sa_data: [c_char; 14],
}

pub struct TcpSocket {
    fd: i32,
}

impl TcpSocket {
    pub fn new() -> OrtResult<Self> {
        let fd = unsafe { socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0) };
        if fd == -1 {
            return ort_err("libc::socket failed");
        }
        set_tcp_fastopen(fd);
        Ok(TcpSocket { fd })
    }

    pub fn connect(&self, addr: &SocketAddrV4) -> OrtResult<()> {
        let c_addr = socket_addr_v4_to_c(addr);
        let len = size_of::<sockaddr_in>() as socklen_t;
        let res = unsafe { connect(self.fd, &c_addr as *const _ as *const sockaddr, len) };
        if res == -1 {
            return ort_err("connect failed");
        }
        Ok(())
    }
}

impl Read for TcpSocket {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        let bytes_read = unsafe { read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
        if bytes_read < 0 {
            ort_err("syscall read error")
        } else {
            Ok(bytes_read as usize)
        }
    }
}

impl Write for TcpSocket {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written = unsafe { write(self.fd, buf.as_ptr() as *const c_void, buf.len()) };
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
        setsockopt(
            fd,
            IPPROTO_TCP,
            TCP_FASTOPEN,
            &optval as *const _ as *const core::ffi::c_void,
            size_of::<i32>() as u32,
        );
    }
}

fn socket_addr_v4_to_c(addr: &SocketAddrV4) -> sockaddr_in {
    sockaddr_in {
        sin_family: AF_INET as sa_family_t,
        sin_port: addr.port().to_be(),
        sin_addr: ip_v4_addr_to_c(addr.ip()),
        ..unsafe { core::mem::zeroed() }
    }
}
fn ip_v4_addr_to_c(addr: &Ipv4Addr) -> in_addr {
    // `s_addr` is stored as BE on all machines and the array is in BE order.
    // So the native endian conversion method is used so that it's never swapped.
    in_addr {
        s_addr: u32::from_ne_bytes(addr.octets()),
    }
}

unsafe extern "C" {
    fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
    fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int;
    pub fn setsockopt(
        socket: c_int,
        level: c_int,
        name: c_int,
        value: *const c_void,
        option_len: socklen_t,
    ) -> c_int;

    fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t;
    fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t;
}
