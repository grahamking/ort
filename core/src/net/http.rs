//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use core::net::SocketAddr;

extern crate alloc;
use alloc::ffi::CString;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::libc;
use crate::{
    ErrorKind, OrtError, OrtResult, Read, TcpSocket, TlsStream, Write, common::buf_read, ort_error,
    ort_from_err,
};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const HOST: &str = "openrouter.ai";
const EXPECTED_HTTP_200: &str = "HTTP/1.1 200 OK";
const CHUNKED_HEADER: &str = "Transfer-Encoding: chunked";

pub fn list_models(api_key: &str, addrs: Vec<SocketAddr>) -> OrtResult<TlsStream<TcpSocket>> {
    let tcp = connect(addrs)?;
    let mut tls = TlsStream::connect(tcp, HOST)?;

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

    tls.write_all(prefix.as_bytes())
        .map_err(|e| ort_from_err(ErrorKind::SocketWriteFailed, "write list_models request", e))?;
    tls.flush()
        .map_err(|e| ort_from_err(ErrorKind::SocketWriteFailed, "flush list_models request", e))?;

    Ok(tls)
}

pub fn chat_completions(
    api_key: &str,
    addrs: Vec<SocketAddr>,
    json_body: &str,
) -> OrtResult<buf_read::OrtBufReader<TlsStream<TcpSocket>>> {
    let tcp = connect(addrs)?;

    let mut tls = TlsStream::connect(tcp, HOST)?;

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

    tls.write_all(prefix.as_bytes()).map_err(|e| {
        ort_from_err(
            ErrorKind::SocketWriteFailed,
            "write chat_completions header",
            e,
        )
    })?;
    tls.write_all(body).map_err(|e| {
        ort_from_err(
            ErrorKind::SocketWriteFailed,
            "write chat_completions body",
            e,
        )
    })?;
    tls.flush()
        .map_err(|e| ort_from_err(ErrorKind::SocketWriteFailed, "flush chat_completions", e))?;

    Ok(buf_read::OrtBufReader::new(tls))
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

impl core::error::Error for HttpError {}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.status_line, self.body)
    }
}

impl From<HttpError> for OrtError {
    fn from(err: HttpError) -> OrtError {
        let c_s = CString::new("\nHTTP ERROR: ".to_string() + &err.to_string()).unwrap();
        unsafe {
            libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
        }
        ort_error(ErrorKind::HttpStatusError, "")
    }
}

/// Advances the reader to point to the first line of the body.
/// Returns true if the body has transfer encoding chunked, and hence needs
/// special handling.
pub fn skip_header<T: Read + Write>(
    reader: &mut buf_read::OrtBufReader<TlsStream<T>>,
) -> Result<bool, HttpError> {
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
fn connect(addrs: Vec<SocketAddr>) -> OrtResult<TcpSocket> {
    // TODO: Erorr handling, don't just try the first

    //let mut errs = vec![];
    //let addrs: Vec<_> = addrs.to_socket_addrs().unwrap().collect();
    for addr in addrs {
        let addr_v4 = match addr {
            SocketAddr::V4(v4) => v4,
            _ => continue,
        };
        let sock = TcpSocket::new()?;
        sock.connect(&addr_v4)?;
        return Ok(sock);
        /*
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(tcp) => {
                set_tcp_fastopen(&tcp);
                return Ok(tcp);
            }
            Err(err) => {
                errs.push((addr, err));
            }
        }
        */
    }
    /*
    let err_msg: Vec<String> = errs
        .into_iter()
        .map(|(addr, err)| format!("Failed connecting to {addr:?}: {err}"))
        .collect();
    Err(io::Error::other(err_msg.join("; ")))
    */
    Err(ort_error(
        ErrorKind::HttpConnectError,
        "connect error handling TODO",
    ))
}
