//!  Error types that can occur while scanning crate sources.

use std::error::Error;
use std::fmt;
use std::io::Error as IoError;

/// Errors that can occur while scanning crate sources.
#[derive(Debug)]
pub enum ScanError {
    /// No entry point was found. Should never happen, since you can't upload a crate to crates.io without one.
    MissingEntryPoint,
    /// A source file could not be read.
    Io(IoError),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntryPoint => write!(f, "Entry point is not found"),
            Self::Io(err) => write!(f, "Failed to read source file: {err}"),
        }
    }
}

impl Error for ScanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::MissingEntryPoint => None,
        }
    }
}

impl From<IoError> for ScanError {
    fn from(err: IoError) -> Self {
        Self::Io(err)
    }
}

/// Errors that can occur while parsing a Cargo.lock dependency tree.
#[derive(Debug, PartialEq, Eq)]
pub enum TreeError {
    /// The lockfile contains no packages.
    Empty,
    /// A `[[package]]` block is missing its name or version.
    MissingField,
    /// A dependency does not match any package in the lockfile.
    UnresolvedDependency(String),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "lockfile contains no packages"),
            Self::MissingField => write!(f, "package block is missing `name` or `version`"),
            Self::UnresolvedDependency(dep) => {
                write!(f, "dependency `{dep}` does not match any package in the lockfile")
            }
        }
    }
}

impl Error for TreeError {}
