//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

extern crate alloc;

use alloc::borrow::{Borrow, Cow};
use alloc::string::String;

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum ErrorKind {
    // Configuration & arguments
    //
    MissingApiKey = 1,
    // Argument parse error
    InvalidArguments,
    // Failed to parse config
    ConfigParseFailed,
    // Failed to read config file
    ConfigReadFailed,
    MissingHomeDir,
    MissingSystemPrompt,
    // Run `date` to substitute in system prompt
    FailedFillingSystemPrompt,

    // Conversation/history
    //
    HistoryMissing,
    HistoryParseFailed,
    HistoryReadFailed,
    HistoryLookupFailed,

    // Input validation
    //
    InvalidMessageSchema,
    ParsingToolCallParams,
    ToolDoesNotExist,

    // Output & streaming
    //
    StdoutWriteFailed,
    // OpenRouter did not return usage stats
    MissingUsageStats,
    ResponseStreamError,
    LastWriterError,

    // Filesystem
    //
    FileCreateFailed,
    FileReadFailed,
    FileWriteFailed,
    FileStatFailed,
    DirOpenFailed,

    // Networking
    //
    DnsResolveFailed,
    ReadingResolvConfFailed,
    // libc::socket failed
    SocketCreateFailed,
    // libc::connect failed
    SocketConnectFailed,
    SocketReadFailed,
    SocketWriteFailed,

    // Generic I/O
    //
    UnexpectedEof,
    // O_NONBLOCK socket has no data to read right now
    WouldBlock,

    // HTTP chunked transfer decoding
    //
    // EOF while reading chunk size
    ChunkedEofInSize,
    // Error reading chunk size
    ChunkedSizeReadError,
    ChunkedInvalidSize,
    // Error reading chunked data line
    ChunkedDataReadError,

    // HTTP / higher-level protocol
    //
    HttpStatusError,
    HttpConnectError,

    // TLS handshake / record processing
    //
    TlsExpectedHandshakeRecord,
    TlsExpectedServerHello,
    // Expected server to send dummy Change Cipher Spec
    TlsExpectedChangeCipherSpec,
    TlsExpectedEncryptedRecords,
    TlsBadHandshakeFragment,
    TlsFinishedVerifyFailed,
    TlsUnsupportedCipher,
    TlsAlertReceived,
    TlsRecordTooShort,
    TlsHandshakeHeaderTooShort,
    TlsHandshakeBodyTooShort,
    TlsServerHelloTooShort,
    TlsServerHelloSessionIdInvalid,
    TlsServerHelloExtTooShort,
    TlsExtensionHeaderTooShort,
    TlsExtensionLengthInvalid,
    TlsKeyShareServerHelloInvalid,
    TlsServerGroupUnsupported,
    TlsKeyShareLengthInvalid,
    TlsServerNotTls13,
    TlsMissingServerKey,
    TlsAes128GcmDecryptFailed,

    // Time
    //
    TscCpuidLeafUnavailable,
    TscInvalidCalibration,
    TscMissingCrystalClock,

    // Misc
    //
    FormatError,
    RateLimited,
    Other,
}

impl ErrorKind {
    pub fn as_string(&self) -> String {
        alloc::format!("{self:?}")
    }
}

pub type OrtResult<T> = Result<T, OrtError>;

#[derive(Clone, Debug)]
pub struct OrtError {
    pub kind: ErrorKind,
    pub context: Cow<'static, str>,
}

pub fn ort_error(kind: ErrorKind, context: &'static str) -> OrtError {
    ort_err(kind, context.into())
}

pub fn ort_err(kind: ErrorKind, context: Cow<'static, str>) -> OrtError {
    OrtError { kind, context }
}

impl OrtError {
    // On error, main calls and prints this right before exiting
    pub fn as_string(&self) -> String {
        let k = self.kind.as_string();
        let mut out = String::with_capacity(k.len() + 2 + self.context.len());
        out.push_str(&k);
        out.push_str(": ");
        out.push_str(self.context.borrow());
        out
    }

    #[cfg(debug_assertions)]
    pub fn debug_print(&self) {
        use crate::{syscall, utils::zclean};
        use alloc::ffi::CString;
        let mut s = self.as_string();
        let c_s = CString::new(zclean(&mut s)).unwrap();
        syscall::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
    }

    #[cfg(not(debug_assertions))]
    pub fn debug_print(&self) {}
}

/*
impl From<&'static str> for OrtError {
    fn from(err: &'static str) -> OrtError {
        ort_err(ErrorKind::Other, err.into())
    }
}
*/

pub trait Context<T, E> {
    /// Wrap the error value with additional context.
    fn context(self, context: &'static str) -> OrtResult<T>;
    fn context_msg(self, context: String) -> OrtResult<T>;
}

impl<T, E> Context<T, E> for Result<T, E>
where
    E: Into<OrtError>,
{
    /// Wrap the error value with additional context.
    fn context(self, context: &'static str) -> OrtResult<T> {
        ctx(self, context.into())
    }
    fn context_msg(self, context: String) -> OrtResult<T> {
        ctx(self, context.into())
    }
}

fn ctx<T, E: Into<OrtError>>(
    result: Result<T, E>,
    context: Cow<'static, str>,
) -> Result<T, OrtError> {
    match result {
        Ok(ok) => Ok(ok),
        Err(error) => {
            let mut err: OrtError = error.into();
            if err.context.is_empty() {
                err.context = context;
            } else {
                let new_ctx = err.context.into_owned() + " in " + context.borrow();
                err.context = new_ctx.into();
            }
            Err(err)
        }
    }
}
