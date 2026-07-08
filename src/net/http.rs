//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

use core::net::SocketAddr;

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{
    Context, ErrorKind, OrtError, OrtResult, Read, TcpSocket, TlsStream, Write, common::buf_read,
    common::io::ReadLine, ort_error,
};
use crate::{syscall, utils};

/// How long to wait for 'connect' syscall.
/// Everyone blackholes the initial SYN so we can't wait for a rejection.
/// Default timeout is about 2 minutes. Change to 2 seconds.
const SOCKET_CONNECT_TIMEOUT_MS: i32 = 2000;

const EXPECTED_HTTP_200: &str = "HTTP/1.1 200 OK";
const CHUNKED_HEADER: &str = "Transfer-Encoding: chunked";
const CONTENT_LENGTH_0: &str = "Content-Length: 0";

const POST: &[u8] = "POST ".as_bytes();
const GET: &[u8] = "GET ".as_bytes();
const HTTP_1_1: &[u8] = " HTTP/1.1\r\n".as_bytes();
const HOST_HEADER: &[u8] = "Host: ".as_bytes();
const CONTENT_LENGTH_HEADER: &[u8] = "Content-Length: ".as_bytes();
const CRLF: &[u8] = "\r\n".as_bytes();

// The constant part of the list request headers
const LIST_REQ_MIDDLE: &[u8] = concat!(
    "Accept: application/json\r\n",
    "User-Agent: ",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    "\r\n",
    "HTTP-Referer: https://github.com/grahamking/ort\r\n",
    "X-Title: ort\r\n",
    "Authorization: Bearer "
)
.as_bytes();

pub fn list_models(
    api_key: &str,
    base_url: &str,
    addrs: Vec<SocketAddr>,
) -> OrtResult<TlsStream<TcpSocket>> {
    let tcp = connect(addrs)?;

    let (host, _port, base_path) = split_url(base_url);
    let mut tls = TlsStream::connect(tcp, host)?;

    // Built request on the stack, zero alloc
    // Req is about 276 bytes right now. 384 is 256 + 128.
    let mut req = [0u8; 384];

    // GET <list_url> HTTP/1.1\r\n
    let list_url = base_path.to_string() + "/models";
    let mut start = 0;
    let mut end = GET.len();
    req[start..end].copy_from_slice(GET);
    start = end;
    end += list_url.len();
    req[start..end].copy_from_slice(list_url.as_bytes());
    start = end;
    end += HTTP_1_1.len();
    req[start..end].copy_from_slice(HTTP_1_1);

    // Host: <host>\r\n
    start = end;
    end += HOST_HEADER.len();
    req[start..end].copy_from_slice(HOST_HEADER);
    start = end;
    end += host.len();
    req[start..end].copy_from_slice(host.as_bytes());
    start = end;
    end += CRLF.len();
    req[start..end].copy_from_slice(CRLF);

    // Rest of the HTTP headers
    start = end;
    end += LIST_REQ_MIDDLE.len();
    req[start..end].copy_from_slice(LIST_REQ_MIDDLE);

    // The constant part finished with "Authorization: Bearer ".
    // Append the API key and the final double CRLF.
    start = end;
    end += api_key.len();
    req[start..end].copy_from_slice(api_key.as_bytes());
    start = end;
    end += CRLF.len();
    req[start..end].copy_from_slice(CRLF);
    start = end;
    end += CRLF.len();
    req[start..end].copy_from_slice(CRLF);

    tls.write_all(&req[..end])
        .context("write list_models request")?;
    tls.flush().context("flush list_models request")?;

    Ok(tls)
}

const CHAT_REQ_MIDDLE: &[u8] = concat!(
    "Content-Type: application/json\r\n",
    "Accept: text/event-stream\r\n",
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
)
.as_bytes();

pub fn chat_completions(
    api_key: &str,
    base_url: &str,
    addrs: Vec<SocketAddr>,
    json_body: &str,
) -> OrtResult<buf_read::OrtBufReader<TlsStream<TcpSocket>>> {
    let tcp = connect(addrs)?;

    let (host, _port, base_path) = split_url(base_url);
    let mut tls = TlsStream::connect(tcp, host)?;

    let body = json_body.as_bytes();

    // Built HTTP request header on the stack.
    // With longest current model name headers len is 341.
    let mut req = [0u8; 512];

    // POST <chat_completions_url> HTTP/1.1\r\n
    let chat_completions_url = base_path.to_string() + "/chat/completions";
    let mut start = 0;
    let mut end = POST.len();
    req[start..end].copy_from_slice(POST);
    start = end;
    end += chat_completions_url.len();
    req[start..end].copy_from_slice(chat_completions_url.as_bytes());
    start = end;
    end += HTTP_1_1.len();
    req[start..end].copy_from_slice(HTTP_1_1);

    // Host: <host>\r\n
    start = end;
    end += HOST_HEADER.len();
    req[start..end].copy_from_slice(HOST_HEADER);
    start = end;
    end += host.len();
    req[start..end].copy_from_slice(host.as_bytes());
    start = end;
    end += CRLF.len();
    req[start..end].copy_from_slice(CRLF);

    // Content-Length: <body-len>\r\n
    start = end;
    end += CONTENT_LENGTH_HEADER.len();
    req[start..end].copy_from_slice(CONTENT_LENGTH_HEADER);
    let mut body_len_buf: [u8; 16] = [0; 16];
    let buf_len = utils::to_ascii(body.len(), &mut body_len_buf[..]);
    start = end;
    // Subtract two to strip the \n and \0 that to_ascii adds
    end += buf_len - 2;
    req[start..end].copy_from_slice(&body_len_buf[..buf_len - 2]);
    start = end;
    end += CRLF.len();
    req[start..end].copy_from_slice(CRLF);

    // Rest of the HTTP headers
    start = end;
    end += CHAT_REQ_MIDDLE.len();
    req[start..end].copy_from_slice(CHAT_REQ_MIDDLE);

    // The constant part finished with "Authorization: Bearer ".
    // Append the API key and the final double CRLF.
    start = end;
    end += api_key.len();
    req[start..end].copy_from_slice(api_key.as_bytes());
    start = end;
    end += CRLF.len();
    req[start..end].copy_from_slice(CRLF);
    start = end;
    end += CRLF.len();
    req[start..end].copy_from_slice(CRLF);

    //let end_str = utils::num_to_string(end);
    //utils::print_string(c"REQ LEN ", &end_str);

    tls.write_all(&req[..end])
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
        let mut msg = String::with_capacity(16 + self.status_line.len() + self.body.len());
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
        syscall::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
        ort_error(ErrorKind::HttpStatusError, "")
    }
}

/// Advances the reader to point to the first line of the body.
/// Returns true if the body has transfer encoding chunked, and hence needs
/// special handling.
pub fn skip_header<T: Read + Write>(
    reader: &mut buf_read::OrtBufReader<TlsStream<T>>,
) -> Result<bool, HttpError> {
    let mut buffer = String::with_capacity(512);
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

/// Attempt to connect to all the SocketAddr in order.
/// The addresses come from the system resolver or `${XDG_CONFIG_HOME}/ort.json`
/// in settings/dns.
fn connect(addrs: Vec<SocketAddr>) -> OrtResult<TcpSocket> {
    for addr in addrs {
        let addr_v4 = match addr {
            SocketAddr::V4(v4) => v4,
            _ => continue,
        };
        let sock = TcpSocket::new()?;
        if sock.connect(&addr_v4, SOCKET_CONNECT_TIMEOUT_MS).is_ok() {
            return Ok(sock);
        }
    }
    Err(ort_error(
        ErrorKind::HttpConnectError,
        "'connect' failed on all of the IP addresses",
    ))
}

/// "https://openrouter.ai:443/api/v1" => ("openrouter.ai", 443, "/api/v1")
fn split_url(base_url: &str) -> (&str, u16, &str) {
    let mut host_start = 0;
    let mut host_end = base_url.len();

    let mut port_start = 0;
    let mut port_end = 0;

    let mut base_path_start = 0;
    let base_path_end = base_url.len();

    if base_url.starts_with("https://") {
        host_start += "https://".len();
        base_path_start = host_start;
    }
    if let Some(ps) = base_url[host_start..host_end].find(":") {
        host_end = host_start + ps;
        port_start = host_start + ps + 1;
    }
    if let Some(path_start) = base_url[base_path_start..base_path_end].find("/") {
        if port_start == 0 {
            // If there's no port, the host stops where the path starts
            host_end = base_path_start + path_start;
        }
        port_end = base_path_start + path_start;
        base_path_start += path_start;
    }

    let host = &base_url[host_start..host_end];
    // TODO parse from port_start..port_end
    // Extract string to num from common/json_parser.json parse_u32 into common/utils
    let port = 443;
    let base_path = &base_url[base_path_start..base_path_end];
    (host, port, base_path)
}

#[cfg(test)]
mod tests {
    #[test]
    pub fn test_split_url() {
        let input = "https://openrouter.ai:443/api/v1";
        let (host, port, base) = super::split_url(input);
        assert_eq!(host, "openrouter.ai");
        assert_eq!(port, 443);
        assert_eq!(base, "/api/v1");

        let input = "https://openrouter.ai/api/v1";
        let (host, port, base) = super::split_url(input);
        assert_eq!(host, "openrouter.ai");
        assert_eq!(port, 443);
        assert_eq!(base, "/api/v1");

        let input = "openrouter.ai/api/v1";
        let (host, port, base) = super::split_url(input);
        assert_eq!(host, "openrouter.ai");
        assert_eq!(port, 443);
        assert_eq!(base, "/api/v1");
    }
}
