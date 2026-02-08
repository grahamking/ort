//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::net::SocketAddr;

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{
    Context, ErrorKind, OrtError, OrtResult, Read, TcpSocket, TlsStream, Write, common::buf_read,
    ort_error,
};
use crate::{libc, utils};

const EXPECTED_HTTP_200: &str = "HTTP/1.1 200 OK";
const CHUNKED_HEADER: &str = "Transfer-Encoding: chunked";
const CONTENT_LENGTH_0: &str = "Content-Length: 0";

const LIST_REQ_PREFIX: &str = concat!(
    "GET {LIST_URL} HTTP/1.1\r\n",
    "Accept: application/json\r\n",
    "Host: {HOST}\r\n",
    "User-Agent: ",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    "\r\n",
    "HTTP-Referer: https://github.com/grahamking/ort\r\n",
    "X-Title: ort\r\n",
    "Authorization: Bearer "
);

const CHAT_REQ_PREFIX: &str = concat!(
    "POST {CHAT_COMPLETIONS_URL} HTTP/1.1\r\n",
    "Content-Type: application/json\r\n",
    "Accept: text/event-stream\r\n",
    "Host: {HOST}\r\n",
    "User-Agent: ",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    "\r\n",
    // ID for openrouter.ai App rankings
    "HTTP-Referer: https://github.com/grahamking/ort\r\n",
    // Name to appear in openrouter.ai App rankings
    "X-Title: ort\r\n",
    "Authorization: Bearer "
);

pub fn list_models(
    api_key: &str,
    host: &'static str,
    list_url: &'static str,
    addrs: Vec<SocketAddr>,
) -> OrtResult<TlsStream<TcpSocket>> {
    let tcp = connect(addrs)?;
    let mut tls = TlsStream::connect(tcp, host)?;

    let with_sub = LIST_REQ_PREFIX
        .replace("{LIST_URL}", list_url)
        .replace("{HOST}", host);
    let mut req = String::with_capacity(with_sub.len() + 128);
    req.push_str(&with_sub);
    // The prefix finished with "Authorization: Bearer ". Append the API key
    // and the final double CRLF.
    req.push_str(api_key);
    req.push_str("\r\n\r\n");

    //utils::print_string(c"Request header:", &req);

    tls.write_all(req.as_bytes())
        .context("write list_models request")?;
    tls.flush().context("flush list_models request")?;

    Ok(tls)
}

pub fn chat_completions(
    api_key: &str,
    host: &'static str,
    chat_completions_url: &'static str,
    addrs: Vec<SocketAddr>,
    json_body: &str,
) -> OrtResult<buf_read::OrtBufReader<TlsStream<TcpSocket>>> {
    let tcp = connect(addrs)?;

    let mut tls = TlsStream::connect(tcp, host)?;

    // 2) Write HTTP/1.1 request
    let body = json_body.as_bytes();
    let mut len_buf: [u8; 16] = [0; 16];
    let str_len = utils::to_ascii(body.len(), &mut len_buf[..]);

    let with_sub = CHAT_REQ_PREFIX
        .replace("{CHAT_COMPLETIONS_URL}", chat_completions_url)
        .replace("{HOST}", host);
    let mut req = String::with_capacity(with_sub.len() + 128);
    req.push_str(&with_sub);
    req.push_str(api_key);
    req.push_str("\r\nContent-Length: ");
    // Subtract two to strip the \n and \0 that to_ascii adds
    req.push_str(unsafe { str::from_utf8_unchecked(&len_buf[..str_len - 2]) });
    req.push_str("\r\n\r\n");

    //utils::print_string(c"Request header:", &req);
    //utils::print_string(c"Request bdy   :", json_body);

    tls.write_all(req.as_bytes())
        .context("write chat_completions header")?;
    tls.write_all(body).context("write chat_completions body")?;
    tls.flush().context("flush chat_completions")?;

    Ok(buf_read::OrtBufReader::new(tls))
}

#[derive(Debug)]
pub struct HttpError {
    status_line: String,
    body: String,
}

impl HttpError {
    pub(crate) fn as_string(&self) -> String {
        let mut msg = String::with_capacity(15 + self.status_line.len() + self.body.len());
        msg.push_str("\nHTTP ERROR: ");
        msg.push_str(&self.status_line);
        msg.push_str(", ");
        msg.push_str(&self.body);
        msg.push('\0');
        msg
    }

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

impl From<HttpError> for OrtError {
    fn from(err: HttpError) -> OrtError {
        let c_s = unsafe { CString::from_vec_with_nul_unchecked(err.as_string().into_bytes()) };
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
            return Err(HttpError::status(
                "Internal TLS error: ".to_string() + &err.as_string(),
            ));
        }
    };
    let status = status.trim();
    //utils::print_string(c"HTTP response status: ", status);

    // Skip the rest of the headers
    let mut is_chunked = false;
    let mut has_content = true;
    buffer.clear();
    loop {
        reader.read_line(&mut buffer).map_err(|err| {
            HttpError::status("Reading response header: ".to_string() + &err.as_string())
        })?;
        let header = buffer.trim();
        if header.is_empty() {
            // end of headers
            break;
        }
        if header == CHUNKED_HEADER {
            is_chunked = true;
        }
        if header == CONTENT_LENGTH_0 {
            has_content = false;
        }
        //utils::print_string(c"HTTP response header: ", header);

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
        if !has_content {
            return Err(HttpError::status(status.to_string()));
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
    //let err_msg: Vec<String> = errs
    //    .into_iter()
    //    .map(|(addr, err)| format!("Failed connecting to {addr:?}: {err}"))
    //    .collect();
    //Err(io::Error::other(err_msg.join("; ")))
    Err(ort_error(
        ErrorKind::HttpConnectError,
        "connect error handling TODO",
    ))
}
