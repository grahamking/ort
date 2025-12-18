//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

#![allow(non_camel_case_types)]

use core::{
    ffi::{c_char, c_int, c_long, c_uchar, c_uint, c_ushort, c_void},
    mem::MaybeUninit,
};

type size_t = usize;
type ssize_t = isize;
type clockid_t = c_int;
type time_t = i64;
type ino_t = u64;
type off_t = i64;
type dev_t = u64;
type nlink_t = u64;
type mode_t = u32;
type uid_t = u32;
type gid_t = u32;
type blksize_t = i64;
type blkcnt_t = i64;

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

pub const DT_REG: u8 = 8;

pub const PROT_READ: c_int = 1;
pub const PROT_WRITE: c_int = 2;

pub const MAP_PRIVATE: c_int = 0x0002;
pub const MAP_ANONYMOUS: c_int = 0x0020;
pub const MAP_STACK: c_int = 0x020000;

pub const CLONE_VM: c_int = 0x100;
pub const CLONE_FS: c_int = 0x200;
pub const CLONE_FILES: c_int = 0x400;

pub const SIGCHLD: c_int = 17;

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

#[repr(C)]
pub struct dirent {
    pub d_ino: ino_t,
    pub d_off: off_t,
    pub d_reclen: c_ushort,
    pub d_type: c_uchar,
    pub d_name: [c_char; 256],
}

#[repr(C)]
pub struct stat {
    pub st_dev: dev_t,
    pub st_ino: ino_t,
    pub st_nlink: nlink_t,
    pub st_mode: mode_t,
    pub st_uid: uid_t,
    pub st_gid: gid_t,
    __pad0: Padding<c_int>,
    pub st_rdev: dev_t,
    pub st_size: off_t,
    pub st_blksize: blksize_t,
    pub st_blocks: blkcnt_t,
    pub st_atime: time_t,
    pub st_atime_nsec: i64,
    pub st_mtime: time_t,
    pub st_mtime_nsec: i64,
    pub st_ctime: time_t,
    pub st_ctime_nsec: i64,
    __unused: Padding<[i64; 3]>,
}

// Opaque C data structure, only used as pointer type
pub enum DIR {}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct Padding<T: Copy>(MaybeUninit<T>);

impl<T: Copy> Default for Padding<T> {
    fn default() -> Self {
        Self(MaybeUninit::zeroed())
    }
}

unsafe extern "C" {
    pub fn syscall(num: c_long, ...) -> c_long;

    pub fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t;
    pub fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t;

    pub fn open64(path: *const c_char, oflag: c_int, ...) -> c_int;
    pub fn open(path: *const c_char, mode: c_int) -> c_int;
    pub fn access(path: *const c_char, mode: c_int) -> c_int;
    pub fn close(fd: c_int) -> c_int;
    pub fn stat(path: *const c_char, buf: *mut stat) -> c_int;

    pub fn mkdir(path: *const c_char, mode: u32) -> c_int;
    pub fn getenv(name: *const c_char) -> *const c_char;

    pub fn opendir(dirname: *const c_char) -> *mut DIR;
    pub fn readdir(dirp: *mut DIR) -> *mut dirent;
    pub fn closedir(dirp: *mut DIR) -> c_int;

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

    pub fn mmap(
        addr: *mut c_void,
        len: size_t,
        prot: c_int,
        flags: c_int,
        fd: c_int,
        offset: off_t,
    ) -> *mut c_void;

    pub fn clone(
        cb: extern "C" fn(*mut c_void) -> c_int,
        child_stack: *mut c_void,
        flags: c_int,
        arg: *mut c_void,
        ...
    ) -> c_int;
}
