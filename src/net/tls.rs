//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::marker::PhantomData;
use std::io::{Read as _, Write as _};
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::net::socket::TcpSocket;
use crate::{ErrorKind, OrtResult, Read, Write, ort_error};

pub struct TlsStream<T = TcpSocket> {
    inner: StreamOwned<ClientConnection, std::net::TcpStream>,
    _transport: PhantomData<T>,
}

impl TlsStream<TcpSocket> {
    pub fn connect(tcp: TcpSocket, host: &'static str) -> OrtResult<Self> {
        let config = client_config()?;
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|_| ort_error(ErrorKind::TlsHandshakeFailed, "invalid server name"))?;
        let conn = ClientConnection::new(config, server_name)
            .map_err(|_| ort_error(ErrorKind::TlsHandshakeFailed, "client connection"))?;
        Ok(TlsStream {
            inner: StreamOwned::new(conn, tcp.into_inner()),
            _transport: PhantomData,
        })
    }
}

impl<T> Read for TlsStream<T> {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        self.inner
            .read(buf)
            .map_err(|_| ort_error(ErrorKind::TlsReadFailed, "tls read failed"))
    }
}

impl<T> Write for TlsStream<T> {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        self.inner
            .write(buf)
            .map_err(|_| ort_error(ErrorKind::TlsWriteFailed, "tls write failed"))
    }

    fn flush(&mut self) -> OrtResult<()> {
        self.inner
            .flush()
            .map_err(|_| ort_error(ErrorKind::TlsWriteFailed, "tls flush failed"))
    }
}

fn client_config() -> OrtResult<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs();
    let (added, _ignored) = roots.add_parsable_certificates(certs.certs);
    if added == 0 {
        return Err(ort_error(
            ErrorKind::TlsHandshakeFailed,
            "no usable root certificates",
        ));
    }

    let provider = rustls::crypto::ring::default_provider();
    let config = ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .map_err(|_| ort_error(ErrorKind::TlsHandshakeFailed, "tls versions"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}
