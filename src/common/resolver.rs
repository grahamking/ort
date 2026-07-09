//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025, 2026 Graham King
//!

use core::{net::Ipv4Addr, ptr::copy_nonoverlapping};

extern crate alloc;
use alloc::vec::Vec;

use crate::{
    ErrorKind, OrtResult, ort_error,
    syscall::{self, AF_INET, SOCK_DGRAM},
    utils,
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

/// This is only called if .config/ort.json's settings/dns is not set. Which you should set.
/// # Safety
/// System programming is for everyone
pub unsafe fn resolve(host: &str) -> OrtResult<Vec<Ipv4Addr>> {
    let label: &[u8] = &host_to_dns_label(host);
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
            s_addr: resolver_ip_address()?,
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

    // Each response is 16 bytes. The last four are the IP. Parse from the end.
    let answer_count = u16::from_le_bytes([buf[7], buf[8]]);
    let mut result = Vec::with_capacity(answer_count as usize);
    for i in 0..answer_count as usize {
        let record_end = bytes_read as usize - (16 * i);
        let ip = u32::from_be_bytes([
            buf[record_end - 4],
            buf[record_end - 3],
            buf[record_end - 2],
            buf[record_end - 1],
        ]);
        result.push(Ipv4Addr::from_bits(ip));
        /*
        let b0 = crate::utils::num_to_string(buf[record_end - 4]);
        let b1 = crate::utils::num_to_string(buf[record_end - 3]);
        let b2 = crate::utils::num_to_string(buf[record_end - 2]);
        let b3 = crate::utils::num_to_string(buf[record_end - 1]);
        let ip_str = b0 + "." + &b1 + "." + &b2 + "." + &b3;
        crate::utils::print_string(c"Got IP: ", &ip_str);
        */
    }
    result.reverse();
    Ok(result)
}

// Converts a host string into a DNS label which is each component (split on '.')
// prefixed by it's length, with a null bytes at the end.
// Example:
//  Input: "openrouter.ai"
//  Output: vec![10, b'o', b'p', b'e', b'n', b'r', b'o', b'u', b't', b'e', b'r',
//               2, b'a', b'i',
//               0]
fn host_to_dns_label(host: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(host.len() + 3);
    for part in host.split(".") {
        out.push(part.len() as u8);
        out.extend_from_slice(part.as_bytes());
    }
    out.push(0);
    out
}

fn resolver_ip_address() -> OrtResult<u32> {
    // /etc/resolv.conf is POSIX so error if it doesn't exist.
    let resolv_conf = utils::filename_read_to_string("/etc/resolv.conf").map_err(|_err| {
        ort_error(
            ErrorKind::ReadingResolvConfFailed,
            "Err reading resolv.conf",
        )
    })?;

    // We only look at the first `nameserver` entry, there can be up to three.
    let mut nameserver = None;
    for line in resolv_conf.lines() {
        if let Some(ns) = line.strip_prefix("nameserver ") {
            nameserver = ip_str_to_u32(ns);
            break;
        }
    }

    // Default to 127.0.0.53
    Ok(nameserver.unwrap_or_else(|| u32::from_ne_bytes([127, 0, 0, 53])))
}

/// Convert a string IPv4 such as "127.0.0.53" to u32
/// Nice work from deepseek-v4-flash.
fn ip_str_to_u32(s: &str) -> Option<u32> {
    s.split('.')
        .try_fold((0u32, 0u8), |(acc, i), x| {
            Some(((acc << 8) | x.parse::<u8>().ok()? as u32, i + 1))
        })
        .and_then(|(v, i)| (i == 4).then_some(v.to_be()))
}

#[cfg(test)]
mod tests {
    #[test]
    pub fn test_host_to_dns_label() {
        let input = "openrouter.ai";
        let expected = &[
            10, b'o', b'p', b'e', b'n', b'r', b'o', b'u', b't', b'e', b'r', 2, b'a', b'i', 0,
        ];
        let output = super::host_to_dns_label(input);
        assert_eq!(output, expected);

        let input = "integrate.api.nvidia.com";
        let expected = &[
            9, b'i', b'n', b't', b'e', b'g', b'r', b'a', b't', b'e', 3, b'a', b'p', b'i', 6, b'n',
            b'v', b'i', b'd', b'i', b'a', 3, b'c', b'o', b'm', 0,
        ];
        let output = super::host_to_dns_label(input);
        assert_eq!(output, expected);

        let input = "localhost";
        let expected = &[9, b'l', b'o', b'c', b'a', b'l', b'h', b'o', b's', b't', 0];
        let output = super::host_to_dns_label(input);
        assert_eq!(output, expected);
    }
}
