//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

#![allow(non_camel_case_types)]

use core::ffi::{c_char, c_int, c_long, c_uint, c_void};

type size_t = usize;
type ssize_t = isize;
type clockid_t = c_int;
type time_t = i64;

pub type socklen_t = u32;
pub type sa_family_t = u16;
pub type in_addr_t = u32;
pub type in_port_t = u16;

pub const O_CLOEXEC: c_int = 0x80000;
pub const O_RDONLY: c_int = 0;
pub const O_WRONLY: c_int = 1;
//const O_RDWR: c_int = 2;
pub const O_CREAT: c_int = 64;
pub const O_TRUNC: c_int = 512;

pub const F_OK: i32 = 0;

pub const FUTEX_WAIT: c_int = 0;
pub const FUTEX_WAKE: c_int = 1;
pub const SYS_FUTEX: c_long = 202; // asm/unistd_64.h __NR_futex

pub const SOCK_STREAM: c_int = 1;
pub const SOCK_CLOEXEC: c_int = O_CLOEXEC;
pub const AF_INET: c_int = 2;
pub const IPPROTO_TCP: i32 = 6;
pub const TCP_FASTOPEN: i32 = 23;

pub const CLOCK_MONOTONIC: clockid_t = 1;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct sigset_t {
    __val: [u64; 16],
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct sigaction {
    pub sa_sigaction: usize,
    pub sa_mask: sigset_t,
    pub sa_flags: i32,
    pub sa_restorer: Option<extern "C" fn()>,
}

#[repr(C)]
pub struct in_addr {
    pub s_addr: in_addr_t,
}

#[repr(C)]
pub struct sockaddr_in {
    pub sin_family: sa_family_t,
    pub sin_port: in_port_t,
    pub sin_addr: in_addr,
    pub sin_zero: [u8; 8],
}

#[repr(C)]
pub struct sockaddr {
    pub sa_family: sa_family_t,
    pub sa_data: [c_char; 14],
}

#[repr(C)]
pub struct addrinfo {
    pub ai_flags: c_int,
    pub ai_family: c_int,
    pub ai_socktype: c_int,
    pub ai_protocol: c_int,
    pub ai_addrlen: socklen_t,
    pub ai_addr: *mut sockaddr_in,
    pub ai_canonname: *mut c_char,
    pub ai_next: *mut addrinfo,
}

#[repr(C)]
pub struct timespec {
    pub tv_sec: time_t,
    pub tv_nsec: c_long,
}

unsafe extern "C" {
    pub fn syscall(num: c_long, ...) -> c_long;

    pub fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t;
    pub fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t;

    pub fn open64(path: *const c_char, oflag: c_int, ...) -> c_int;
    pub fn open(path: *const c_char, mode: c_int) -> c_int;
    pub fn access(path: *const c_char, mode: c_int) -> c_int;
    pub fn close(fd: c_int) -> c_int;

    pub fn mkdir(path: *const c_char, mode: u32) -> c_int;
    pub fn getenv(name: *const c_char) -> *const c_char;

    pub fn sigemptyset(set: *mut sigset_t) -> i32;
    pub fn sigaction(signum: i32, act: *const sigaction, oldact: *mut sigaction) -> i32;

    pub fn getaddrinfo(
        node: *const c_char,
        service: *const c_char,
        hints: *const addrinfo,
        res: *mut *mut addrinfo,
    ) -> c_int;
    pub fn freeaddrinfo(res: *mut addrinfo);

    pub fn getrandom(buf: *mut c_void, buflen: usize, flags: c_uint) -> isize;

    pub fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
    pub fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int;
    pub fn setsockopt(
        socket: c_int,
        level: c_int,
        name: c_int,
        value: *const c_void,
        option_len: socklen_t,
    ) -> c_int;

    pub fn clock_gettime(clock_id: clockid_t, tp: *mut timespec) -> c_int;
}
