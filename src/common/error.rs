//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::string::String;

#[repr(u8)]
#[derive(Clone, Copy)]
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
    TlsAes128GcmDecryptFailed,

    // Misc
    FormatError,
    RateLimited,
    Other,
}

impl ErrorKind {
    pub fn as_string(&self) -> &'static str {
        match self {
            ErrorKind::MissingApiKey => "MissingApiKey",
            ErrorKind::InvalidArguments => "InvalidArguments",
            ErrorKind::ConfigParseFailed => "ConfigParseFailed",
            ErrorKind::ConfigReadFailed => "ConfigReadFailed",
            ErrorKind::MissingHomeDir => "MissingHomeDir",
            ErrorKind::HistoryMissing => "HistoryMissing",
            ErrorKind::HistoryParseFailed => "HistoryParseFailed",
            ErrorKind::HistoryReadFailed => "HistoryReadFailed",
            ErrorKind::HistoryLookupFailed => "HistoryLookupFailed",
            ErrorKind::InvalidMessageSchema => "InvalidMessageSchema",
            ErrorKind::StdoutWriteFailed => "StdoutWriteFailed",
            ErrorKind::QueueDesync => "QueueDesync",
            ErrorKind::MissingUsageStats => "MissingUsageStats",
            ErrorKind::ResponseStreamError => "ResponseStreamError",
            ErrorKind::LastWriterError => "LastWriterError",
            ErrorKind::FileCreateFailed => "FileCreateFailed",
            ErrorKind::FileReadFailed => "FileReadFailed",
            ErrorKind::FileWriteFailed => "FileWriteFailed",
            ErrorKind::FileStatFailed => "FileStatFailed",
            ErrorKind::DirOpenFailed => "DirOpenFailed",
            ErrorKind::ThreadStackAllocFailed => "ThreadStackAllocFailed",
            ErrorKind::ThreadSpawnFailed => "ThreadSpawnFailed",
            ErrorKind::DnsResolveFailed => "DnsResolveFailed",
            ErrorKind::SocketCreateFailed => "SocketCreateFailed",
            ErrorKind::SocketConnectFailed => "SocketConnectFailed",
            ErrorKind::SocketReadFailed => "SocketReadFailed",
            ErrorKind::SocketWriteFailed => "SocketWriteFailed",
            ErrorKind::UnexpectedEof => "UnexpectedEof",
            ErrorKind::ChunkedEofInSize => "ChunkedEofInSize",
            ErrorKind::ChunkedSizeReadError => "ChunkedSizeReadError",
            ErrorKind::ChunkedInvalidSize => "ChunkedInvalidSize",
            ErrorKind::ChunkedDataReadError => "ChunkedDataReadError",
            ErrorKind::HttpStatusError => "HttpStatusError",
            ErrorKind::HttpConnectError => "HttpConnectError",
            ErrorKind::TlsExpectedHandshakeRecord => "TlsExpectedHandshakeRecord",
            ErrorKind::TlsExpectedServerHello => "TlsExpectedServerHello",
            ErrorKind::TlsExpectedChangeCipherSpec => "TlsExpectedChangeCipherSpec",
            ErrorKind::TlsExpectedEncryptedRecords => "TlsExpectedEncryptedRecords",
            ErrorKind::TlsBadHandshakeFragment => "TlsBadHandshakeFragment",
            ErrorKind::TlsFinishedVerifyFailed => "TlsFinishedVerifyFailed",
            ErrorKind::TlsUnsupportedCipher => "TlsUnsupportedCipher",
            ErrorKind::TlsAlertReceived => "TlsAlertReceived",
            ErrorKind::TlsRecordTooShort => "TlsRecordTooShort",
            ErrorKind::TlsHandshakeHeaderTooShort => "TlsHandshakeHeaderTooShort",
            ErrorKind::TlsHandshakeBodyTooShort => "TlsHandshakeBodyTooShort",
            ErrorKind::TlsServerHelloTooShort => "TlsServerHelloTooShort",
            ErrorKind::TlsServerHelloSessionIdInvalid => "TlsServerHelloSessionIdInvalid",
            ErrorKind::TlsServerHelloExtTooShort => "TlsServerHelloExtTooShort",
            ErrorKind::TlsExtensionHeaderTooShort => "TlsExtensionHeaderTooShort",
            ErrorKind::TlsExtensionLengthInvalid => "TlsExtensionLengthInvalid",
            ErrorKind::TlsKeyShareServerHelloInvalid => "TlsKeyShareServerHelloInvalid",
            ErrorKind::TlsServerGroupUnsupported => "TlsServerGroupUnsupported",
            ErrorKind::TlsKeyShareLengthInvalid => "TlsKeyShareLengthInvalid",
            ErrorKind::TlsServerNotTls13 => "TlsServerNotTls13",
            ErrorKind::TlsMissingServerKey => "TlsMissingServerKey",
            ErrorKind::TlsAes128GcmDecryptFailed => "TlsAes128GcmDecryptFailed",
            ErrorKind::FormatError => "FormatError",
            ErrorKind::RateLimited => "RateLimited",
            ErrorKind::Other => "Other",
        }
    }
}

pub type OrtResult<T> = Result<T, OrtError>;

#[derive(Clone, Copy)]
pub struct OrtError {
    pub kind: ErrorKind,
    pub context: &'static str,
}

pub fn ort_error(kind: ErrorKind, context: &'static str) -> OrtError {
    OrtError { kind, context }
}

impl OrtError {
    pub fn as_string(&self) -> String {
        let k = self.kind.as_string();
        let mut out = String::with_capacity(k.len() + 2 + self.context.len());
        out.push_str(k);
        out.push_str(": ");
        out.push_str(self.context);
        out
    }

    #[cfg(debug_assertions)]
    pub fn debug_print(&self) {
        use crate::{libc, utils::zclean};
        use alloc::ffi::CString;
        let mut s = self.as_string();
        let c_s = CString::new(zclean(&mut s)).unwrap();
        unsafe {
            libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
        }
    }

    #[cfg(not(debug_assertions))]
    pub fn debug_print(&self) {}
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
