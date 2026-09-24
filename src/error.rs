//! Error types.

use std::error::Error;
use std::fmt;
use std::io::Error as IoError;

/// Errors that can occur while scanning crate sources.
#[derive(Debug)]
pub enum IsSafeError {
    /// No entry point was found. Should never happen, since you can't upload a crate to crates.io without one.
    MissingEntryPoint,
    /// A source file could not be read.
    Io(IoError),
}

impl fmt::Display for IsSafeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntryPoint => write!(f, "Entry point is not found"),
            Self::Io(err) => write!(f, "Failed to read source file: {err}"),
        }
    }
}

impl Error for IsSafeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::MissingEntryPoint => None,
        }
    }
}

impl From<IoError> for IsSafeError {
    fn from(err: IoError) -> Self {
        Self::Io(err)
    }
}
