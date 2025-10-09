//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fmt;
use std::io::{self, BufRead as _, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const HOST: &str = "openrouter.ai";
const EXPECTED_HTTP_200: &str = "HTTP/1.1 200 OK";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

pub fn list_models<A: ToSocketAddrs>(
    api_key: &str,
    addrs: A,
) -> io::Result<BufReader<crate::tls::TlsStream>> {
    let tcp = connect(addrs)?;
    let mut tls = crate::tls::TlsStream::connect(tcp, HOST).map_err(io::Error::other)?;

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
) -> io::Result<BufReader<crate::tls::TlsStream>> {
    let tcp = connect(addr)?;
    //tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
    //tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

    let mut tls = crate::tls::TlsStream::connect(tcp, HOST).map_err(io::Error::other)?;

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

/// Consume the reader, returning either a Lines reader pointing at the body
/// if HTTP status 200, or an error if status other than 200.
pub fn read_header(
    reader: BufReader<crate::tls::TlsStream>,
) -> Result<impl Iterator<Item = Result<String, io::Error>>, HttpError> {
    let mut response_lines = reader.lines();
    let status = match response_lines.next() {
        Some(Ok(status)) => status,
        Some(Err(err)) => {
            return Err(HttpError::status(format!("Internal TLS error: {err}")));
        }
        None => {
            return Err(HttpError::status("Missing initial status line".to_string()));
        }
    };

    // Skip to the content
    let mut response_lines = response_lines
        // Skip the rest of the headers
        .skip_while(|line| line.as_ref().map(|l| l.trim().len()).unwrap_or(0) > 0)
        // Then skip until the content
        .skip_while(|line| line.as_ref().map(|l| l.trim().len()).unwrap_or(0) < 5);

    if status.trim() == EXPECTED_HTTP_200 {
        return Ok(response_lines);
    }

    // Usually the body explains the error so gather that.
    match response_lines.next() {
        Some(Ok(err)) => {
            // TODO parse JSON. It looks like this:
            // {"error":{"message":"openai/gpt-oss-90b is not a valid model ID","code":400},"user_id":"user_30mJ0GpP57Kj9wLQ4mDCfMS5nk0"}
            Err(HttpError::new(status, err))
        }
        _ => Err(HttpError::status(status)),
    }
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
