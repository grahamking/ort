//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King
//!

#![allow(non_camel_case_types)]
#![allow(clippy::upper_case_acronyms)]

extern crate alloc;
use alloc::{ffi::CString, string::String, vec::Vec};

use crate::{ErrorKind, OrtResult, ort_error};
use core::{
    arch::asm,
    ffi::{CStr, c_char, c_int, c_long, c_short, c_uchar, c_ushort, c_void},
    mem::MaybeUninit,
};

pub type size_t = usize;
type ssize_t = isize;
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
type pid_t = c_int;

pub type socklen_t = u32;
pub type sa_family_t = u16;
pub type in_addr_t = u32;
pub type in_port_t = u16;

// /usr/include/linux/limits.h
const NAME_MAX: usize = 255;

// /usr/include/asm/unistd_64.h
const SYS_READ: u32 = 0;
const SYS_WRITE: u32 = 1;
const SYS_OPEN: u32 = 2;
const SYS_CLOSE: u32 = 3;
const SYS_FSTAT: u32 = 5;
const SYS_POLL: u32 = 7;
const SYS_MMAP: u32 = 9;
const SYS_MPROTECT: u32 = 10;
const SYS_IOCTL: u32 = 16;
const SYS_ACCESS: u32 = 21;
const SYS_DUP2: i32 = 33;
const SYS_SOCKET: u32 = 41;
const SYS_CONNECT: u32 = 42;
const SYS_SETSOCKOPT: i32 = 54;
const SYS_GETSOCKOPT: i32 = 55;
const SYS_FORK: i32 = 57;
const SYS_EXECVE: i32 = 59;
const SYS_EXIT: i32 = 60;
const SYS_WAIT4: i32 = 61;
const SYS_FCNTL: i32 = 72;
const SYS_MKDIR: u32 = 83;
const SYS_EPOLL_CREATE: i32 = 213;
const SYS_INOTIFY_ADD_WATCH: i32 = 254;
const SYS_EPOLL_WAIT: i32 = 232;
const SYS_EPOLL_CTL: i32 = 233;
const SYS_GETDENTS64: u32 = 217;
const SYS_PIPE2: i32 = 293;
const SYS_INOTIFY_INIT1: i32 = 294;

pub const EAGAIN: i32 = -11; // Operation would block, try again
const EINTR: i32 = -4; // Interrupted system call
const EACCES: i32 = -13; // Permission denied
const ENOTTY: i32 = -25; // Not a typewriter / inappropriate ioctl for device
pub const EINPROGRESS: i32 = -115; // Operation now in progress

// TODO check these two, might be wrong values, and convert to decimal
pub const O_CLOEXEC: c_int = 0x80000;
pub const O_DIRECTORY: c_int = 0x10000;

pub const O_RDONLY: c_int = 0;
pub const O_WRONLY: c_int = 1;
//const O_RDWR: c_int = 2;
pub const O_CREAT: c_int = 64;
pub const O_TRUNC: c_int = 512;
pub const O_NONBLOCK: c_int = 2048;

pub const F_OK: i32 = 0;

pub const SOCK_STREAM: c_int = 1;
pub const SOCK_DGRAM: c_int = 2;
pub const SOCK_CLOEXEC: c_int = O_CLOEXEC;
pub const AF_INET: c_int = 2;
pub const SOL_SOCKET: c_int = 1;
pub const SO_ERROR: c_int = 4;
pub const IPPROTO_TCP: i32 = 6;
pub const TCP_FASTOPEN_CONNECT: i32 = 30;
pub const EPOLLIN: u32 = 0x001;
pub const EPOLL_CTL_ADD: c_int = 1;
pub const IN_MOVED_TO: u32 = 0x00000080;
//pub const IN_MODIFY: u32 = 0x00000002; // File was modified
pub const IN_CLOSE_WRITE: u32 = 0x00000008; // Writable file was closed
const IN_MASK_CREATE: u32 = 0x10000000;

pub const DT_REG: u8 = 8;

pub const PROT_NONE: c_int = 0;
pub const PROT_READ: c_int = 1;
pub const PROT_WRITE: c_int = 2;

pub const MAP_PRIVATE: c_int = 0x0002;
pub const MAP_ANONYMOUS: c_int = 0x0020;
pub const MAP_STACK: c_int = 0x020000;

pub const F_GETFL: c_int = 3;
pub const F_SETFL: c_int = 4;
const TCGETS: usize = 0x5401;
const POLLIN: c_short = 0x001;
const POLLOUT: c_short = 0x004;

pub struct ProcessOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u32,
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

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct epoll_event {
    pub events: u32,
    pub data: u64,
}

#[repr(C)]
pub struct inotify_event {
    pub wd: c_int,                // Watch descriptor
    pub mask: u32,                // Mask describing event
    pub cookie: u32,              // Unique cookie associating related events (for rename(2))
    pub len: u32,                 // Size of name field
    pub name: [c_char; NAME_MAX], // Optional null-terminated name
}

#[repr(C)]
struct pollfd {
    fd: c_int,
    events: c_short,
    revents: c_short,
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

pub fn isatty(fd: c_int) -> bool {
    let mut ret: c_long;
    let mut termios = MaybeUninit::<[u8; 64]>::uninit();
    unsafe {
        asm!("syscall",
            inlateout("rax") SYS_IOCTL as c_long => ret,
            in("edi") fd,
            in("rsi") TCGETS,
            in("rdx") termios.as_mut_ptr(),
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    match ret {
        0 => true,
        x if x == ENOTTY as c_long => false,
        _ => false,
    }
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

fn poll(fds: *mut pollfd, nfds: size_t, timeout: c_int) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_POLL => ret,
            in("rdi") fds,
            in("rsi") nfds,
            in("edx") timeout,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn poll_write(fd: c_int, timeout_ms: c_int) -> c_int {
    let mut fds = [pollfd {
        fd,
        events: POLLOUT,
        revents: 0,
    }];
    poll(fds.as_mut_ptr(), 1, timeout_ms)
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

pub fn socket(domain: c_int, ty: c_int, protocol: c_int) -> i32 {
    let mut ret: i32;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_SOCKET => ret,
            in("edi") domain,
            in("esi") ty,
            in("edx") protocol,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_CONNECT => ret,
            in("edi") socket,
            in("rsi") address,
            in("edx") len,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn setsockopt(
    socket: c_int,
    level: c_int,
    name: c_int,
    value: *const c_void,
    option_len: socklen_t,
) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_SETSOCKOPT => ret,
            in("edi") socket,
            in("esi") level,
            in("edx") name,
            in("r10") value,
            in("r8d") option_len,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn getsockopt(
    socket: c_int,
    level: c_int,
    name: c_int,
    value: *mut c_void,
    option_len: *mut socklen_t,
) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_GETSOCKOPT => ret,
            in("edi") socket,
            in("esi") level,
            in("edx") name,
            in("r10") value,
            in("r8") option_len,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn fcntl(fd: c_int, op: c_int, flags: c_int) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_FCNTL => ret,
            in("edi") fd,
            in("esi") op,
            in("edx") flags,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn inotify_init1(flags: c_int) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_INOTIFY_INIT1 => ret,
            in("edi") flags,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_INOTIFY_ADD_WATCH => ret,
            in("edi") fd,
            in("rsi") path,
            in("edx") mask | IN_MASK_CREATE,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn epoll_create(size: c_int) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_EPOLL_CREATE => ret,
            in("edi") size,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn epoll_ctl(epfd: c_int, op: c_int, fd: c_int, event: *mut epoll_event) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_EPOLL_CTL => ret,
            in("edi") epfd,
            in("esi") op,
            in("edx") fd,
            in("r10") event,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn epoll_wait(
    epfd: c_int,
    events: *mut epoll_event,
    maxevents: c_int,
    timeout: c_int,
) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_EPOLL_WAIT => ret,
            in("edi") epfd,
            in("rsi") events,
            in("edx") maxevents,
            in("r10d") timeout,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn pipe2(pipefd: *mut c_int, flags: c_int) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_PIPE2 => ret,
            in("rdi") pipefd,
            in("esi") flags,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn dup2(oldfd: c_int, newfd: c_int) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_DUP2 => ret,
            in("edi") oldfd,
            in("esi") newfd,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline(never)]
pub fn fork() -> pid_t {
    let mut ret: pid_t;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_FORK => ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn execve(
    path: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    let mut ret: c_int;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_EXECVE => ret,
            in("rdi") path,
            in("rsi") argv,
            in("rdx") envp,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

fn find_bash(envp: *const *const c_char) -> OrtResult<CString> {
    const DEFAULT_PATH: &[u8] = b"/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    const BASH: &[u8] = b"bash";

    let env_path = path_from_envp(envp);
    let path = env_path.as_deref().unwrap_or(DEFAULT_PATH);

    for dir in path.split(|b| *b == b':') {
        let mut candidate = Vec::with_capacity(dir.len() + BASH.len() + 1);
        if !dir.is_empty() {
            candidate.extend_from_slice(dir);
            candidate.push(b'/');
        }
        candidate.extend_from_slice(BASH);

        if let Ok(cpath) = CString::new(candidate)
            && access(cpath.as_ptr(), F_OK) == 0
        {
            return Ok(cpath);
        }
    }

    Err(ort_error(ErrorKind::Other, "system bash not found"))
}

pub fn waitpid(pid: pid_t, status: *mut c_int, options: c_int) -> pid_t {
    let mut ret: pid_t;
    unsafe {
        asm!("syscall",
            inout("eax") SYS_WAIT4 => ret,
            in("edi") pid,
            in("rsi") status,
            in("edx") options,
            in("r10") core::ptr::null_mut::<c_void>(),
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

pub fn system(command: &str) -> OrtResult<ProcessOutput> {
    const STDOUT_FILENO: c_int = 1;
    const STDERR_FILENO: c_int = 2;

    let command = CString::new(command)
        .map_err(|_| ort_error(ErrorKind::Other, "system command contains nul byte"))?;
    let (env_bytes, envp) = current_envp();
    let bash_path = find_bash(envp.as_ptr())?;

    let mut stdout_pipe = [0 as c_int; 2];
    if pipe2(stdout_pipe.as_mut_ptr(), O_CLOEXEC) < 0 {
        return Err(ort_error(ErrorKind::Other, "system pipe2 failed"));
    }

    let mut stderr_pipe = [0 as c_int; 2];
    if pipe2(stderr_pipe.as_mut_ptr(), O_CLOEXEC) < 0 {
        let _ = close(stdout_pipe[0]);
        let _ = close(stdout_pipe[1]);
        return Err(ort_error(ErrorKind::Other, "system pipe2 failed"));
    }

    let pid = fork();
    if pid < 0 {
        let _ = close(stdout_pipe[0]);
        let _ = close(stdout_pipe[1]);
        let _ = close(stderr_pipe[0]);
        let _ = close(stderr_pipe[1]);
        return Err(ort_error(ErrorKind::Other, "system fork failed"));
    }

    if pid == 0 {
        let _ = close(stdout_pipe[0]);
        let _ = close(stderr_pipe[0]);
        if dup2(stdout_pipe[1], STDOUT_FILENO) < 0 {
            exit(127);
        }
        if dup2(stderr_pipe[1], STDERR_FILENO) < 0 {
            exit(127);
        }
        if stdout_pipe[1] != STDOUT_FILENO && stdout_pipe[1] != STDERR_FILENO {
            let _ = close(stdout_pipe[1]);
        }
        if stderr_pipe[1] != STDOUT_FILENO && stderr_pipe[1] != STDERR_FILENO {
            let _ = close(stderr_pipe[1]);
        }

        let argv = [
            c"bash".as_ptr(),
            c"-c".as_ptr(),
            command.as_ptr(),
            core::ptr::null(),
        ];
        let _ = execve(bash_path.as_ptr(), argv.as_ptr(), envp.as_ptr());
        exit(127);
    }

    let _ = close(stdout_pipe[1]);
    let _ = close(stderr_pipe[1]);

    // The child stays in the parent's foreground process group, so terminal
    // Ctrl-C is delivered to the shell by the kernel's default job control.
    let (stdout, stderr) = match read_child_pipes(stdout_pipe[0], stderr_pipe[0]) {
        Ok(output) => output,
        Err(err) => {
            let _ = wait_for_child(pid);
            return Err(err);
        }
    };

    let exit_code = wait_for_child(pid)?;

    // Keep the environment backing storage alive until after the child execs.
    let _ = env_bytes.len();
    let _ = bash_path.as_bytes().len();

    Ok(ProcessOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    })
}

fn read_child_pipes(stdout_fd: c_int, stderr_fd: c_int) -> OrtResult<(Vec<u8>, Vec<u8>)> {
    let mut fds = [
        pollfd {
            fd: stdout_fd,
            events: POLLIN,
            revents: 0,
        },
        pollfd {
            fd: stderr_fd,
            events: POLLIN,
            revents: 0,
        },
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut buf = [0u8; 4096];
    let mut open_fds = 2;

    while open_fds > 0 {
        let num_ready = poll(fds.as_mut_ptr(), fds.len(), -1);
        if num_ready < 0 {
            if num_ready == EINTR {
                continue;
            }
            close_poll_fds(&mut fds);
            return Err(ort_error(ErrorKind::Other, "system poll failed"));
        }

        for idx in 0..fds.len() {
            if fds[idx].fd < 0 || fds[idx].revents == 0 {
                continue;
            }

            let output = if idx == 0 { &mut stdout } else { &mut stderr };
            match read_ready_fd(fds[idx].fd, output, &mut buf) {
                Ok(true) => {
                    let _ = close(fds[idx].fd);
                    fds[idx].fd = -1;
                    open_fds -= 1;
                }
                Ok(false) => {
                    fds[idx].revents = 0;
                }
                Err(err) => {
                    close_poll_fds(&mut fds);
                    return Err(err);
                }
            }
        }
    }

    Ok((stdout, stderr))
}

fn read_ready_fd(fd: c_int, output: &mut Vec<u8>, buf: &mut [u8; 4096]) -> OrtResult<bool> {
    let bytes_read = read(fd, buf.as_mut_ptr().cast(), buf.len());
    if bytes_read > 0 {
        output.extend_from_slice(&buf[..bytes_read as usize]);
        Ok(false)
    } else if bytes_read == 0 {
        Ok(true)
    } else if bytes_read == EINTR {
        Ok(false)
    } else {
        Err(ort_error(ErrorKind::Other, "system read failed"))
    }
}

fn close_poll_fds(fds: &mut [pollfd; 2]) {
    for fd in fds {
        if fd.fd >= 0 {
            let _ = close(fd.fd);
            fd.fd = -1;
        }
    }
}

fn wait_for_child(pid: pid_t) -> OrtResult<u32> {
    let mut status = 0;
    loop {
        let ret = waitpid(pid, &mut status, 0);
        if ret == pid {
            return Ok(exit_code_from_wait_status(status));
        }
        if ret == EINTR {
            continue;
        }
        return Err(ort_error(ErrorKind::Other, "system waitpid failed"));
    }
}

fn exit_code_from_wait_status(status: c_int) -> u32 {
    let signal = status & 0x7f;
    if signal == 0 {
        ((status >> 8) & 0xff) as u32
    } else {
        (128 + signal) as u32
    }
}

fn path_from_envp(envp: *const *const c_char) -> Option<Vec<u8>> {
    if envp.is_null() {
        return None;
    }

    let mut entry = envp;
    unsafe {
        while !(*entry).is_null() {
            let bytes = CStr::from_ptr(*entry).to_bytes();
            if let Some(path) = bytes.strip_prefix(b"PATH=") {
                return Some(path.to_vec());
            }
            entry = entry.add(1);
        }
    }
    None
}

fn current_envp() -> (Vec<u8>, Vec<*const c_char>) {
    let mut env_bytes = read_proc_environ();
    if env_bytes.last().is_some_and(|b| *b != 0) {
        env_bytes.push(0);
    }

    let mut envp = Vec::new();
    let mut start = 0usize;
    for idx in 0..env_bytes.len() {
        if env_bytes[idx] == 0 {
            if idx > start {
                envp.push(env_bytes[start..].as_ptr().cast());
            }
            start = idx + 1;
        }
    }
    envp.push(core::ptr::null());

    (env_bytes, envp)
}

fn read_proc_environ() -> Vec<u8> {
    let fd = match open(c"/proc/self/environ".as_ptr(), O_RDONLY | O_CLOEXEC, 0) {
        Ok(fd) => fd,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let bytes_read = read(fd, buf.as_mut_ptr().cast(), buf.len());
        if bytes_read > 0 {
            out.extend_from_slice(&buf[..bytes_read as usize]);
        } else if bytes_read == 0 {
            break;
        } else if bytes_read == EINTR {
            continue;
        } else {
            let _ = close(fd);
            return Vec::new();
        }
    }
    let _ = close(fd);

    out
}

pub fn exit(exit_code: i32) -> ! {
    unsafe {
        asm!("syscall",
            in("eax") SYS_EXIT,
            in("edi") exit_code,
            options(nostack, nomem, noreturn)
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_captures_stdout_stderr_and_exit_code() {
        let out = match super::system("printf out; printf err >&2; exit 7") {
            Ok(out) => out,
            Err(err) => panic!("{}", err.as_string()),
        };
        assert_eq!(out.stdout, "out");
        assert_eq!(out.stderr, "err");
        assert_eq!(out.exit_code, 7);
    }

    /*
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
    */
}
