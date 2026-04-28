//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::net::{IpAddr, Ipv4Addr};
use std::net::ToSocketAddrs;

use crate::{ErrorKind, OrtResult, ort_error};

pub fn resolve(label: &[u8]) -> OrtResult<Ipv4Addr> {
    let host = dns_label_to_host(label)?;
    (host.as_str(), 443)
        .to_socket_addrs()
        .map_err(|_| ort_error(ErrorKind::DnsResolveFailed, "system resolver failed"))?
        .find_map(|addr| match addr.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })
        .ok_or_else(|| ort_error(ErrorKind::DnsResolveFailed, "no IPv4 address found"))
}

fn dns_label_to_host(label: &[u8]) -> OrtResult<String> {
    let mut host = String::new();
    let mut pos = 0;
    while pos < label.len() {
        let len = label[pos] as usize;
        if len == 0 {
            break;
        }
        pos += 1;
        if pos + len > label.len() {
            return Err(ort_error(ErrorKind::DnsResolveFailed, "invalid dns label"));
        }
        if !host.is_empty() {
            host.push('.');
        }
        host.push_str(
            core::str::from_utf8(&label[pos..pos + len])
                .map_err(|_| ort_error(ErrorKind::DnsResolveFailed, "invalid dns label"))?,
        );
        pos += len;
    }
    Ok(host)
}
