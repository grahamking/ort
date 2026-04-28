//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::net::SocketAddr;
use std::io::{Read as _, Write as _};
use std::net::TcpStream as StdTcpStream;
use std::time::Duration;

use crate::{ErrorKind, OrtResult, Read, Write, ort_error};

pub struct TcpSocket {
    inner: StdTcpStream,
}

impl TcpSocket {
    pub fn connect(addr: SocketAddr) -> OrtResult<Self> {
        let inner = StdTcpStream::connect_timeout(&addr, Duration::from_secs(15))
            .map_err(|_| ort_error(ErrorKind::SocketConnectFailed, ""))?;
        let _ = inner.set_nodelay(true);
        Ok(TcpSocket { inner })
    }

    pub(crate) fn into_inner(self) -> StdTcpStream {
        self.inner
    }
}

impl Read for TcpSocket {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        self.inner.read(buf).map_err(|err| {
            if err.kind() == std::io::ErrorKind::WouldBlock {
                ort_error(ErrorKind::WouldBlock, "")
            } else {
                ort_error(ErrorKind::SocketReadFailed, "read error")
            }
        })
    }
}

impl Write for TcpSocket {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        self.inner
            .write(buf)
            .map_err(|_| ort_error(ErrorKind::SocketWriteFailed, "write error"))
    }

    fn flush(&mut self) -> OrtResult<()> {
        self.inner
            .flush()
            .map_err(|_| ort_error(ErrorKind::SocketWriteFailed, "flush error"))
    }
}
