//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::version::TLS13;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const HOST: &str = "openrouter.ai";

pub fn chat_completions<A: ToSocketAddrs>(
    api_key: &str,
    addr: A,
    json_body: &str,
) -> io::Result<Box<dyn BufRead>> {
    let tcp = TcpStream::connect(addr)?;
    tcp.set_nodelay(true)?;

    let root_store = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.into(),
    };

    let cfg = ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
        .with_protocol_versions(&[&TLS13])
        .unwrap()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    //cfg.alpn_protocols = vec![b"http/1.1".to_vec()];

    let server_name = ServerName::try_from(HOST)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid DNS name"))?;
    let conn = ClientConnection::new(Arc::new(cfg), server_name)
        .map_err(|e| io::Error::other(format!("TLS error: {e}")))?;

    // TLS stream
    let mut tls = StreamOwned::new(conn, tcp);

    let body = json_body.as_bytes();
    let prefix = format!(
        concat!(
            "POST /api/v1/chat/completions HTTP/1.1\r\n",
            "Content-Type: application/json\r\n",
            "Accept: text/event-stream\r\n",
            "Host: {}\r\n",
            "Authorization: Bearer {}\r\n",
            "User-Agent: {}\r\n",
            "Content-Length: {}\r\n",
            "\r\n"
        ),
        HOST,
        api_key,
        USER_AGENT,
        body.len()
    );
    tls.write_all(prefix.as_bytes())?;
    tls.write_all(body)?;
    tls.flush()?;

    Ok(Box::new(BufReader::new(tls)))
}
