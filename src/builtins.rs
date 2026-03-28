//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King
//!
//! libc usually provides these

use core::arch::asm;

#[unsafe(no_mangle)]
extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    unsafe {
        asm!(
            "rep movsb",
            inout("rcx") n => _,
            inout("rdi") dest => _,
            inout("rsi") src => _,
            options(nostack, preserves_flags)
        );
    }
    dest
}

#[unsafe(no_mangle)]
extern "C" fn memset(dest: *mut u8, c: i32, count: usize) -> *mut u8 {
    unsafe {
        asm!(
            "rep stosb",
            inout("rcx") count => _,
            inout("rdi") dest => _,
            inout("al") c as u8 => _,
            options(nostack, preserves_flags)
        );
    }
    dest
}

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

#[unsafe(no_mangle)]
extern "C" fn strlen(s: *const i8) -> usize {
    let len: usize;

    unsafe {
        asm!(
            "xor eax, eax",
            "mov rcx, -1",
            "repne scasb",
            "not rcx",
            "dec rcx",
            inout("rdi") s => _,
            lateout("rcx") len,
            lateout("rax") _,
            options(nostack)
        );
    }

    len
}

#[unsafe(no_mangle)]
extern "C" fn bcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    if n == 0 {
        return 0;
    }

    let result: i32;

    unsafe {
        asm!(
            "repe cmpsb",
            "setne al",
            "movzx eax, al",
            inout("rcx") n => _,
            inout("rsi") s1 => _,
            inout("rdi") s2 => _,
            lateout("eax") result,
            options(nostack)
        );
    }

    result
}

#[unsafe(no_mangle)]
extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    if n == 0 {
        return 0;
    }

    let result: i32;

    unsafe {
        asm!(
            "repe cmpsb",
            "je 2f",
            "movzx eax, byte ptr [rsi - 1]",
            "movzx edx, byte ptr [rdi - 1]",
            "sub eax, edx",
            "jmp 3f",
            "2:",
            "xor eax, eax",
            "3:",
            inout("rcx") n => _,
            inout("rsi") s1 => _,
            inout("rdi") s2 => _,
            lateout("eax") result,
            lateout("edx") _,
            options(nostack)
        );
    }

    result
}
