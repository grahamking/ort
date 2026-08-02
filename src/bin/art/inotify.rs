//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King
//!

use core::{
    arch::asm,
    ffi::{c_char, c_int},
};

pub const IN_MOVED_TO: u32 = 0x00000080;
//pub const IN_MODIFY: u32 = 0x00000002; // File was modified
pub const IN_CLOSE_WRITE: u32 = 0x00000008; // Writable file was closed

// /usr/include/linux/limits.h
const NAME_MAX: usize = 255;

const IN_MASK_CREATE: u32 = 0x10000000;
const SYS_INOTIFY_ADD_WATCH: i32 = 254;
const SYS_INOTIFY_INIT1: i32 = 294;

#[repr(C)]
pub struct inotify_event {
    pub wd: c_int,                // Watch descriptor
    pub mask: u32,                // Mask describing event
    pub cookie: u32,              // Unique cookie associating related events (for rename(2))
    pub len: u32,                 // Size of name field
    pub name: [c_char; NAME_MAX], // Optional null-terminated name
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
