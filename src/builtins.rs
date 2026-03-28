//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King
//!
//! libc usually provides these

use core::arch::asm;

#[unsafe(no_mangle)]
extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if n == 0 || core::ptr::eq(dest.cast_const(), src) {
        return dest;
    }

    let dest_addr = dest as usize;
    let src_addr = src as usize;

    unsafe {
        if dest_addr < src_addr || dest_addr.wrapping_sub(src_addr) >= n {
            asm!(
                "rep movsb",
                inout("rcx") n => _,
                inout("rdi") dest => _,
                inout("rsi") src => _,
                options(nostack, preserves_flags)
            );
        } else {
            asm!(
                "std",
                "rep movsb",
                "cld",
                inout("rcx") n => _,
                inout("rdi") dest.add(n - 1) => _,
                inout("rsi") src.add(n - 1) => _,
                options(nostack)
            );
        }
    }

    dest
}
