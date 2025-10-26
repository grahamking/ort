//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fmt::{self, Display};

use crate::{cli::ArgParseError, net::HttpError};

pub type OrtResult<T> = Result<T, OrtError>;

#[derive(Debug, Clone)]
pub struct OrtError {
    msg: String,
    context: Vec<String>,
}

pub fn ort_error<T: Into<String>>(msg: T) -> OrtError {
    OrtError {
        msg: msg.into(),
        context: vec![],
    }
}

pub fn ort_err<X, T: Into<String>>(msg: T) -> Result<X, OrtError> {
    Err(OrtError {
        msg: msg.into(),
        context: vec![],
    })
}

impl std::error::Error for OrtError {}

impl fmt::Display for OrtError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.context.is_empty() {
            write!(f, "{}", self.msg)
        } else {
            write!(
                f,
                "Error: {}. Context: {}",
                self.msg,
                self.context.join(",")
            )
        }
    }
}

impl From<std::io::Error> for OrtError {
    fn from(err: std::io::Error) -> OrtError {
        ort_error(err.to_string())
    }
}

impl From<HttpError> for OrtError {
    fn from(err: HttpError) -> OrtError {
        ort_error(err.to_string())
    }
}

impl From<ArgParseError> for OrtError {
    fn from(err: ArgParseError) -> OrtError {
        ort_error(err.to_string())
    }
}

impl OrtError {
    // Save extra context with this error.
    pub fn context<T: Into<String>>(&mut self, s: T) -> &mut Self {
        self.context.push(s.into());
        self
    }
}

pub trait Context<T, E> {
    /// Wrap the error value with additional context.
    fn context<C>(self, context: C) -> Result<T, OrtError>
    where
        C: Display + Send + Sync + 'static;

    /*
    /// Wrap the error value with additional context that is evaluated lazily
    /// only once an error does occur.
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
    */
}

impl<T, E> Context<T, E> for Result<T, E>
where
    E: Into<OrtError>,
{
    /// Wrap the error value with additional context.
    fn context<C>(self, context: C) -> OrtResult<T>
    where
        C: Display + Send + Sync + 'static,
    {
        match self {
            Ok(ok) => Ok(ok),
            Err(error) => {
                let mut err: OrtError = error.into();
                err.context(context.to_string());
                Err(err)
            }
        }
    }

    /*
    /// Wrap the error value with additional context that is evaluated lazily
    /// only once an error does occur.
    pub fn with_context<C, F>(self, context: F) -> OrtResult<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C
    {
        match self {
            Ok(ok) => Ok(ok),
            Err(error) => Err(error.ext_context(context())),
        }
    }
    */
}
