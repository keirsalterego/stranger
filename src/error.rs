//! One error type for the whole crate.
//!
//! Every parser in `stranger` reports position, because a lockfile that fails
//! to parse is useless feedback without one — "expected ':' at 812:14" is a
//! line you can open, "invalid JSON" is not.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// A parser rejected its input. Line and column are 1-based, and the
    /// column counts characters rather than bytes so that it lines up with
    /// what an editor shows.
    Syntax {
        what: String,
        line: u32,
        col: u32,
    },
    /// A file we were asked to read could not be read.
    Io { path: String, source: std::io::Error },
    /// The command line did not make sense. Exits 2, never 1 — a usage
    /// mistake is not a finding.
    Usage(String),
}

impl Error {
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Error::Io { path: path.into(), source }
    }

    pub fn usage(msg: impl Into<String>) -> Self {
        Error::Usage(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Syntax { what, line, col } => write!(f, "{what} at {line}:{col}"),
            Error::Io { path, source } => write!(f, "{path}: {source}"),
            Error::Usage(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
