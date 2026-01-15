//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::{ffi::c_char, net::Ipv4Addr};

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

use crate::{ErrorKind, OrtResult, libc, ort_error};

/// # Safety
/// System programming is for everyone
pub unsafe fn resolve(host: *const c_char) -> OrtResult<Vec<Ipv4Addr>> {
    let mut hints: libc::addrinfo = unsafe { core::mem::zeroed() };
    hints.ai_family = libc::AF_INET;
    hints.ai_socktype = libc::SOCK_STREAM;
    let mut addr_info = core::ptr::null_mut();
    let return_code = unsafe { libc::getaddrinfo(host, core::ptr::null(), &hints, &mut addr_info) };
    if return_code != 0 {
        return Err(ort_error(
            ErrorKind::DnsResolveFailed,
            "getaddrinfo syscall error",
        ));
    }

    let mut ips = vec![];
    let mut rp = addr_info;
    while !rp.is_null() {
        let ip_bytes = unsafe {
            let addr = (*rp).ai_addr;
            (*addr).sin_addr.s_addr
        };

        let ip = Ipv4Addr::from(ip_bytes.to_ne_bytes());
        ips.push(ip);

        unsafe {
            rp = (*rp).ai_next;
        }
    }
    unsafe { libc::freeaddrinfo(addr_info) };

    Ok(ips)
}
