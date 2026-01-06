//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;

extern crate alloc;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    // Conversation/history
    HistoryMissing,
    HistoryParseFailed,
    HistoryReadFailed,
    HistoryLookupFailed,

    // Input validation
    InvalidMessageSchema,

    // Output & streaming
    //
    StdoutWriteFailed,
    QueueDesync,
    // OpenRouter did not return usage stats
    MissingUsageStats,
    ResponseStreamError,
    LastWriterError,

    // Filesystem
    FileCreateFailed,
    FileReadFailed,
    FileWriteFailed,
    FileStatFailed,
    DirOpenFailed,

    // Threads
    //
    // Failed mmap allocating thread stack
    ThreadStackAllocFailed,
    // pthread_create failed
    ThreadSpawnFailed,

    // Networking
    //
    DnsResolveFailed,
    // libc::socket failed
    SocketCreateFailed,
    // libc::connect failed
    SocketConnectFailed,
    SocketReadFailed,
    SocketWriteFailed,

    // Generic I/O
    UnexpectedEof,

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

    // Misc
    FormatError,
    Other,
}

pub type OrtResult<T> = Result<T, OrtError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrtError {
    pub kind: ErrorKind,
    pub context: &'static str,
}

pub fn ort_error(kind: ErrorKind, context: &'static str) -> OrtError {
    OrtError { kind, context }
}

// In release mode we only have a more general error
#[cfg(not(debug_assertions))]
pub fn ort_from_err<E: core::fmt::Display>(
    kind: ErrorKind,
    context: &'static str,
    _err: E,
) -> OrtError {
    ort_error(kind, context)
}

// In debug mode we print the error. All the generics makes for a larger binary.
#[cfg(debug_assertions)]
pub fn ort_from_err<E: core::fmt::Display>(
    kind: ErrorKind,
    context: &'static str,
    err: E,
) -> OrtError {
    use crate::libc;
    use alloc::ffi::CString;
    use alloc::string::ToString;

    let c_s = CString::new("\nERROR: ".to_string() + &err.to_string()).unwrap();
    unsafe {
        libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
    }

    ort_error(kind, context)
}

impl OrtError {
    #[cfg(debug_assertions)]
    pub fn debug_print(&self) {
        use crate::libc;
        use alloc::ffi::CString;
        use alloc::string::ToString;
        let s = self.to_string();
        let c_s = CString::new(s).unwrap();
        unsafe {
            libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
        }
    }

    #[cfg(not(debug_assertions))]
    pub fn debug_print(&self) {}
}

impl core::error::Error for OrtError {}

impl fmt::Display for OrtError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.context)
    }
}

impl From<core::fmt::Error> for OrtError {
    fn from(err: core::fmt::Error) -> OrtError {
        // fmt::Error has no payload; treat as format error.
        let _ = err;
        ort_error(ErrorKind::FormatError, "")
    }
}

pub trait Context<T, E> {
    /// Wrap the error value with additional context.
    fn context(self, context: &'static str) -> Result<T, OrtError>;
}

impl<T, E> Context<T, E> for Result<T, E>
where
    E: Into<OrtError>,
{
    /// Wrap the error value with additional context.
    fn context(self, context: &'static str) -> OrtResult<T> {
        match self {
            Ok(ok) => Ok(ok),
            Err(error) => {
                let mut err: OrtError = error.into();
                err.context = context;
                Err(err)
            }
        }
    }
}
