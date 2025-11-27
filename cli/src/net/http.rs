//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fmt;
use std::io::{self, BufRead as _, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::fd::AsRawFd as _;
use std::time::Duration;

use super::tls;
use crate::{OrtError, ort_error};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const HOST: &str = "openrouter.ai";
const EXPECTED_HTTP_200: &str = "HTTP/1.1 200 OK";
const CHUNKED_HEADER: &str = "Transfer-Encoding: chunked";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

pub fn list_models<A: ToSocketAddrs>(
    api_key: &str,
    addrs: A,
) -> io::Result<BufReader<tls::TlsStream>> {
    let tcp = connect(addrs)?;
    let mut tls = tls::TlsStream::connect(tcp, HOST).map_err(io::Error::other)?;

    let prefix = format!(
        concat!(
            "GET /api/v1/models HTTP/1.1\r\n",
            "Accept: application/json\r\n",
            "Host: {}\r\n",
            "Authorization: Bearer {}\r\n",
            "User-Agent: {}\r\n",
            "\r\n"
        ),
        HOST, api_key, USER_AGENT,
    );

    tls.write_all(prefix.as_bytes())?;
    tls.flush()?;

    Ok(BufReader::new(tls))
}

pub fn chat_completions<A: ToSocketAddrs>(
    api_key: &str,
    addr: A,
    json_body: &str,
) -> io::Result<BufReader<tls::TlsStream>> {
    let tcp = connect(addr)?;
    //tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
    //tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

    let mut tls = tls::TlsStream::connect(tcp, HOST).map_err(io::Error::other)?;

    // 2) Write HTTP/1.1 request
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

    Ok(BufReader::new(tls))
}

#[derive(Debug)]
pub struct HttpError {
    status_line: String,
    body: String,
}

impl HttpError {
    fn new(status_line: String, body: String) -> Self {
        HttpError { status_line, body }
    }

    fn status(status_line: String) -> Self {
        HttpError {
            status_line,
            body: "".to_string(),
        }
    }
}

impl std::error::Error for HttpError {}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.status_line, self.body)
    }
}

impl From<HttpError> for OrtError {
    fn from(err: HttpError) -> OrtError {
        ort_error(err.to_string())
    }
}

/// Advances the reader to point to the first line of the body.
/// Returns true if the body has transfer encoding chunked, and hence needs
/// special handling.
pub fn skip_header(reader: &mut BufReader<tls::TlsStream>) -> Result<bool, HttpError> {
    let mut buffer = String::with_capacity(16);
    let status = match reader.read_line(&mut buffer) {
        Ok(0) => {
            return Err(HttpError::status("Missing initial status line".to_string()));
        }
        Ok(_) => buffer.clone(),
        Err(err) => {
            return Err(HttpError::status(format!("Internal TLS error: {err}")));
        }
    };
    let status = status.trim();

    // Skip the rest of the headers
    let mut is_chunked = false;
    buffer.clear();
    loop {
        reader
            .read_line(&mut buffer)
            .map_err(|err| HttpError::status(format!("Reading response header: {err}")))?;
        let header = buffer.trim();
        if header.is_empty() {
            // end of headers
            break;
        }
        if header == CHUNKED_HEADER {
            is_chunked = true;
        }
        buffer.clear();
    }

    if status.trim() != EXPECTED_HTTP_200 {
        // Usually the body explains the error so gather that.
        if is_chunked {
            // Skip the size line, the header said transfer encoding chunked
            // so even an HTTP 400 has to respect that.
            let _ = reader.read_line(&mut buffer);
            buffer.clear();
        }
        match reader.read_line(&mut buffer) {
            Ok(_) => {
                // TODO parse JSON. It looks like this:
                // {"error":{"message":"openai/gpt-oss-90b is not a valid model ID","code":400},"user_id":"user_30mJ0GpP57Kj9wLQ4mDCfMS5nk0"}
                return Err(HttpError::new(
                    status.to_string(),
                    buffer.trim().to_string(),
                ));
            }
            _ => return Err(HttpError::status(status.to_string())),
        }
    }
    Ok(is_chunked)
}

/// Attempt to connect to all the SocketAddr in order, with a timeout.
/// The addreses come from the system resolver or `${XDG_CONFIG_HOME}/ort.json`
/// in settings/dns.
fn connect<A: ToSocketAddrs>(addrs: A) -> io::Result<TcpStream> {
    let mut errs = vec![];
    let addrs: Vec<_> = addrs.to_socket_addrs()?.collect();
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(tcp) => {
                set_tcp_fastopen(&tcp);
                return Ok(tcp);
            }
            Err(err) => {
                errs.push((addr, err));
            }
        }
    }
    let err_msg: Vec<String> = errs
        .into_iter()
        .map(|(addr, err)| format!("Failed connecting to {addr:?}: {err}"))
        .collect();
    Err(io::Error::other(err_msg.join("; ")))
}

fn set_tcp_fastopen(tcp: &TcpStream) {
    const IPPROTO_TCP: i32 = 6;
    const TCP_FASTOPEN: i32 = 23;

    let fd = tcp.as_raw_fd();
    let optval: i32 = 1; // Enable
    unsafe {
        setsockopt(
            fd,
            IPPROTO_TCP,
            TCP_FASTOPEN,
            &optval as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

unsafe extern "C" {
    pub fn setsockopt(
        socket: i32,
        level: i32,
        name: i32,
        value: *const core::ffi::c_void,
        option_len: u32,
    ) -> i32;
}
