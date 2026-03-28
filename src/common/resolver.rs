//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::{net::Ipv4Addr, ptr::copy_nonoverlapping};

extern crate alloc;

use crate::{
    ErrorKind, OrtResult,
    syscall::{self, AF_INET, SOCK_DGRAM},
    ort_error, utils,
};

// "openrouter.ai" response is 63 bytes. integrate.api.nvidia.com is more.
const DNS_MAX_PACKET_LEN: usize = 128;
#[rustfmt::skip]
const DNS_PACKET_PREFIX: [u8; 12] = [
    0, 1, // Transaction ID
    1, 0, // Flags (12 bits) and response code. "RD" recursion desired is set.
    0, 1, // Question count
    0, 0, // Answer count
    0, 0, // Authority count
    0, 0, // Additional records count,
];
#[rustfmt::skip]
const DNS_PACKET_SUFFIX: [u8; 4] = [
    0, 1, // Query class: Internet
    0, 1, // Query type: "A" records
];

/// # Safety
/// System programming is for everyone
pub unsafe fn resolve(label: &[u8]) -> OrtResult<Ipv4Addr> {
    // socket
    let sock_fd = syscall::socket(AF_INET, SOCK_DGRAM, 0);
    if sock_fd <= 0 {
        return Err(ort_error(ErrorKind::DnsResolveFailed, "socket failed"));
    }

    // connect
    // In UDP "connect" is really "set peer name".
    // It's a filter so that we only get packets from the correct peer.
    let addr = syscall::sockaddr_in {
        sin_family: AF_INET as u16,
        sin_port: 53_u16.to_be(),
        sin_addr: syscall::in_addr {
            // TODO: Look this up in /etc/resolv.conf
            s_addr: u32::from_ne_bytes([127, 0, 0, 53]),
        },
        sin_zero: [0u8; 8],
    };
    let addr_len = size_of::<syscall::sockaddr_in>() as syscall::socklen_t;

    let res = syscall::connect(
        sock_fd,
        &addr as *const _ as *const syscall::sockaddr,
        addr_len,
    );
    if res < 0 {
        return Err(ort_error(ErrorKind::DnsResolveFailed, "connect failed"));
    }

    // build DNS packet
    let mut query = [0u8; DNS_MAX_PACKET_LEN];
    let mut query_ptr = query.as_mut_ptr();
    unsafe {
        for dat in [&DNS_PACKET_PREFIX, label, &DNS_PACKET_SUFFIX] {
            copy_nonoverlapping(dat.as_ptr(), query_ptr, dat.len());
            query_ptr = query_ptr.add(dat.len());
        }
    }

    // write query
    let bytes_written = syscall::write(sock_fd, query.as_ptr().cast(), query.len());
    if bytes_written != query.len() as i32 {
        return Err(ort_error(ErrorKind::DnsResolveFailed, "write failed"));
    }

    // read response
    let mut buf = [0u8; DNS_MAX_PACKET_LEN];
    let bytes_read = syscall::read(sock_fd, buf.as_mut_ptr().cast(), buf.len());
    if bytes_read <= 0 {
        return Err(ort_error(ErrorKind::DnsResolveFailed, "read failed"));
    }

    // check response code. It's in last four bits of flags. 0 means success.
    let err_code = buf[3] & 0x0F;
    if err_code != 0 {
        let err_code_str = utils::num_to_string(err_code);
        utils::print_string(c"DNS server err code: ", &err_code_str);
        return Err(ort_error(ErrorKind::DnsResolveFailed, "server err code"));
    }

    //let answer_count = u16::from_le_bytes([buf[7], buf[8]]);

    // The last four bytes are always one of the answers, even if there are several
    let end = bytes_read as usize;
    let ip = u32::from_be_bytes([buf[end - 4], buf[end - 3], buf[end - 2], buf[end - 1]]);
    Ok(Ipv4Addr::from_bits(ip))
}
