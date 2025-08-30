//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::net::SocketAddr;

use ureq::{
    config::Config,
    http::Uri,
    unversioned::{
        resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
        transport::NextTimeout,
    },
};

#[derive(Debug)]
pub struct HardcodedResolver {
    addrs: Vec<SocketAddr>,
    inner: DefaultResolver,
}

impl HardcodedResolver {
    pub fn new(ips: &[String]) -> Self {
        let addrs = if ips.is_empty() {
            vec![]
        } else {
            ips.iter()
                .map(|ip| SocketAddr::V4(format!("{ip}:443").parse().unwrap()))
                .collect()
        };
        // The DefaultResolver has no fields, it is very cheap to make
        HardcodedResolver {
            addrs,
            inner: DefaultResolver::default(),
        }
    }
}

impl Resolver for HardcodedResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        if !self.addrs.is_empty() {
            let mut rsa = Resolver::empty(&self.inner);
            for addr in &self.addrs {
                rsa.push(*addr);
            }
            Ok(rsa)
        } else {
            self.inner.resolve(uri, config, timeout)
        }
    }
}
