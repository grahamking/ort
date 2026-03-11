//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

#![allow(non_camel_case_types)]
#![allow(clippy::upper_case_acronyms)]

use core::{
    arch::asm,
    ffi::{c_char, c_int, c_long, c_uchar, c_ushort, c_void},
    mem::MaybeUninit,
};

type c_ulong = u64;
pub type size_t = usize;
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
pub type pthread_t = c_ulong;

pub type socklen_t = u32;
pub type sa_family_t = u16;
pub type in_addr_t = u32;
pub type in_port_t = u16;

// /usr/include/asm/unistd_64.h
const SYS_READ: u32 = 0;
const SYS_WRITE: u32 = 1;
const SYS_OPEN: u32 = 2;
const SYS_CLOSE: u32 = 3;
const SYS_FSTAT: u32 = 5;
const SYS_MMAP: u32 = 9;
const SYS_MPROTECT: u32 = 10;
const SYS_ACCESS: u32 = 21;
const SYS_MKDIR: u32 = 83;
const SYS_GETDENTS64: u32 = 217;
pub const SYS_FUTEX: c_long = 202;

const EACCES: i32 = -13; // Permission denied

pub const O_CLOEXEC: c_int = 0x80000;
pub const O_DIRECTORY: c_int = 0x10000;
pub const O_RDONLY: c_int = 0;
pub const O_WRONLY: c_int = 1;
//const O_RDWR: c_int = 2;
pub const O_CREAT: c_int = 64;
pub const O_TRUNC: c_int = 512;

pub const F_OK: i32 = 0;

pub const FUTEX_WAIT: c_int = 0;
pub const FUTEX_WAKE: c_int = 1;

pub const SOCK_STREAM: c_int = 1;
pub const SOCK_CLOEXEC: c_int = O_CLOEXEC;
pub const AF_INET: c_int = 2;
pub const IPPROTO_TCP: i32 = 6;
pub const TCP_FASTOPEN: i32 = 23;

pub const CLOCK_MONOTONIC: clockid_t = 1;

pub const DT_REG: u8 = 8;

pub const PROT_NONE: c_int = 0;
pub const PROT_READ: c_int = 1;
pub const PROT_WRITE: c_int = 2;

pub const MAP_PRIVATE: c_int = 0x0002;
pub const MAP_ANONYMOUS: c_int = 0x0020;
pub const MAP_STACK: c_int = 0x020000;

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

// /usr/include/bits/dirent.h
#[repr(C)]
pub struct linux_dirent64 {
    pub d_ino: ino_t,
    pub d_off: off_t,
    pub d_reclen: c_ushort,
    pub d_type: c_uchar,
    pub d_name: c_char,
}

// /usr/include/bits/struct_stat.h
// 144 bytes
#[repr(C)]
pub struct Stat {
    pub st_dev: dev_t,         /* Device.  */
    pub st_ino: ino_t,         /* file serial number.	*/
    pub st_nlink: nlink_t,     /* Link count.  */
    pub st_mode: mode_t,       /* File mode.  */
    pub st_uid: uid_t,         /* User ID of the file's owner.  */
    pub st_gid: gid_t,         /* Group ID of the file's group.  */
    __pad0: c_int,             /* switch back to u64 padding */
    pub st_rdev: dev_t,        /* Device number, if device.  */
    pub st_size: off_t,        /* Size of file, in bytes.  */
    pub st_blksize: blksize_t, /* Optimal block size for I/O.  */
    pub st_blocks: blkcnt_t,   /* Number 512-byte blocks allocated. */
    pub st_atime: time_t,
    pub st_atime_nsec: i64,
    pub st_mtime: time_t,
    pub st_mtime_nsec: i64,
    pub st_ctime: time_t,
    pub st_ctime_nsec: i64,
    __unused: [i64; 3],
}

#[repr(C)]
pub struct pthread_attr_t {
    __size: [u64; 7],
}

#[link(name = "c", kind = "dylib")]
unsafe extern "C" {
    pub static mut environ: *mut *mut c_char;

    pub fn syscall(num: c_long, ...) -> c_long;

    pub fn printf(format: *const c_char, ...) -> c_int;
    pub fn isatty(fd: c_int) -> c_int;

    pub fn getenv(name: *const c_char) -> *const c_char;

    pub fn sigemptyset(set: *mut sigset_t) -> i32;

    // #define __NR_rt_sigaction 13
    pub fn sigaction(signum: i32, act: *const sigaction, oldact: *mut sigaction) -> i32;

    pub fn getaddrinfo(
        node: *const c_char,
        service: *const c_char,
        hints: *const addrinfo,
        res: *mut *mut addrinfo,
    ) -> c_int;
    pub fn freeaddrinfo(res: *mut addrinfo);

    // #define __NR_socket 41
    pub fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
    // #define __NR_connect 42
    pub fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int;
    // #define __NR_setsockopt 54
    pub fn setsockopt(
        socket: c_int,
        level: c_int,
        name: c_int,
        value: *const c_void,
        option_len: socklen_t,
    ) -> c_int;

    // #define __NR_clock_gettime 228
    // but use the vDSO instead
    pub fn clock_gettime(clock_id: clockid_t, tp: *mut timespec) -> c_int;

    pub fn pthread_attr_init(attr: *mut pthread_attr_t) -> c_int;
    pub fn pthread_attr_setstack(
        attr: *mut pthread_attr_t,
        stackaddr: *mut c_void,
        stacksize: size_t,
    ) -> c_int;
    pub fn pthread_create(
        native: *mut pthread_t,
        attr: *const pthread_attr_t,
        f: extern "C" fn(*mut c_void) -> *mut c_void,
        value: *mut c_void,
    ) -> c_int;
    pub fn pthread_attr_destroy(attr: *mut pthread_attr_t) -> c_int;
    pub fn pthread_join(native: pthread_t, value: *mut *mut c_void) -> c_int;

    pub fn malloc(size: size_t) -> *mut c_void;
    pub fn calloc(nobj: size_t, size: size_t) -> *mut c_void;
    pub fn realloc(p: *mut c_void, size: size_t) -> *mut c_void;
    pub fn free(p: *mut c_void);
}

// Fill buf with random numbers.
// buf len must be a multiple of 8.
pub fn getrandom(buf: &mut [u8]) {
    debug_assert!(
        buf.len().is_multiple_of(8),
        "getrandom buffer len must be multiple of 8"
    );
    let mut r: u64;
    let mut i = 0;
    while i < buf.len() {
        unsafe {
            asm!("RDRAND rax", out("rax") r);
            buf[i..i + 8].copy_from_slice(&r.to_be_bytes());
        }
        i += 8;
    }
}

// On x86_64 Linux, `syscall` always clobbers rcx and r11.
// Each wrapper must declare both so LLVM does not keep Rust values live there.

pub fn read(fd: c_int, buf: *mut c_void, count: size_t) -> i32 {
    let mut ret: i32;
    unsafe {
        asm!("syscall",
            inlateout("eax") SYS_READ as i32 => ret,
            in("edi") fd,
            in("rsi") buf,
            in("rdx") count,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret
}

pub fn write(fd: c_int, buf: *const c_void, count: size_t) -> i32 {
    let mut ret: i32;
    unsafe {
        asm!("syscall",
            inlateout("eax") SYS_WRITE as i32 => ret,
            in("edi") fd,
            in("rsi") buf,
            in("rdx") count,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret
}

pub fn mmap(
    addr: *mut c_void,
    len: size_t,
    prot: c_int,
    flags: c_int,
    fd: c_int,
    offset: off_t,
) -> *mut c_void {
    let mut ret: isize;
    unsafe {
        asm!("syscall",
            inlateout("rax") SYS_MMAP as isize => ret,
            in("rdi") addr,
            in("rsi") len,
            in("edx") prot,
            in("r10d") flags,
            in("r8d") fd,
            in("r9") offset,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    if ret < 0 {
        core::ptr::null_mut()
    } else {
        ret as *mut c_void
    }
}

pub fn mprotect(addr: *mut c_void, len: size_t, prot: c_int) -> c_int {
    let mut ret: c_long;
    unsafe {
        asm!("syscall",
            inlateout("rax") SYS_MPROTECT as c_long => ret,
            in("rdi") addr,
            in("rsi") len,
            in("edx") prot,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret as c_int
}

pub fn access(path: *const c_char, mode: c_int) -> c_int {
    let mut ret: c_long;
    unsafe {
        asm!("syscall",
            inlateout("rax") SYS_ACCESS as c_long => ret,
            in("rdi") path,
            in("esi") mode,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    if ret < 0 { -1 } else { ret as c_int }
}

pub fn mkdir(path: *const c_char, mode: u32) -> i32 {
    let mut ret: i32;
    unsafe {
        asm!("syscall",
             inout("eax") SYS_MKDIR => ret,
             in("rdi") path,
             in("esi") mode,
             lateout("rcx") _,
             lateout("r11") _,
             options(nostack),
        );
    }
    ret
}

pub fn getdents64(fd: c_int, dirp: *mut c_void, count: size_t) -> ssize_t {
    let mut ret: ssize_t;
    unsafe {
        asm!("syscall",
            inlateout("rax") SYS_GETDENTS64 as ssize_t => ret,
            in("edi") fd,
            in("rsi") dirp,
            in("rdx") count,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    if ret < 0 { -1 } else { ret }
}

pub fn open(path: *const c_char, flags: i32, mode: i32) -> Result<i32, &'static str> {
    let mut result: i32;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_OPEN => result,
            in("rdi") path,
            in("esi") flags,
            in("edx") mode,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    if result == EACCES {
        Err("Permission denied")
    } else if result < 0 {
        Err("SYS_OPEN error")
    } else {
        Ok(result)
    }
}

pub fn close(fd: i32) -> i32 {
    let mut ret: i32;
    unsafe {
        asm!("syscall",
             inout("eax") SYS_CLOSE => ret,
             in("edi") fd,
             lateout("rcx") _,
             lateout("r11") _,
             options(nostack, nomem),
        );
    }
    ret
}

/// open + fstat + close
pub fn stat(path: *const c_char, sb: &mut MaybeUninit<Stat>) -> Result<(), &'static str> {
    let fd = open(path, O_RDONLY, 0)?;
    let mut ret: i32;
    unsafe {
        asm!("syscall",
             inout("eax") SYS_FSTAT => ret,
             in("edi") fd,
             in("rsi") sb as *mut MaybeUninit<Stat>,
             lateout("rcx") _,
             lateout("r11") _,
             options(nostack),
        );
    }
    if ret != 0 {
        Err("fstat failed")
    } else {
        let _ = close(fd);
        Ok(())
    }
}

/*
#[cfg(test)]
mod tests {
    #[test]
    fn test_mkdir() {
        let ret = super::mkdir(c"/home/graham/Temp/HERE_gk_test".as_ptr(), 0o755);
        let s = crate::common::utils::num_to_string(ret);
        crate::common::utils::print_string(c"mkdir ret = ", &s);
    }

    #[test]
    fn test_getrandom() {
        for _ in 0..10 {
            let mut buf = [0u8; 16];
            super::getrandom(&mut buf);
            crate::common::utils::print_hex(c"", &buf);
        }
    }
}
*/
