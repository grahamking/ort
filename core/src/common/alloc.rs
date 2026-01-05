//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use crate::libc;
use core::alloc::Layout;
use core::ffi::c_void;

pub struct LibcAlloc;

unsafe impl core::alloc::GlobalAlloc for LibcAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { libc::malloc(layout.size().max(layout.align())) as *mut u8 }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { libc::calloc(1, layout.size().max(layout.align())) as *mut u8 }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { libc::free(ptr as *mut c_void) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { libc::realloc(ptr as *mut c_void, new_size) as *mut u8 }
    }
}
