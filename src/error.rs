//! One error type for the whole crate.
//!
//! Every parser in `stranger` reports position, because a lockfile that fails
//! to parse is useless feedback without one — "expected ':' at 812:14" is a
//! line you can open, "invalid JSON" is not.

use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    /// A parser rejected its input. Line and column are 1-based, and the
    /// column counts characters rather than bytes so that it lines up with
    /// what an editor shows.
    Syntax { what: String, line: u32, col: u32 },
    /// A syntax error, told which file it came from.
    ///
    /// Separate from `Syntax` because the parsers cannot fill it in: they are
    /// handed a `&str` and never learn a path. See `Error::in_file`.
    InFile { path: String, source: Box<Error> },
    /// A file we were asked to read could not be read.
    Io {
        path: String,
        source: std::io::Error,
    },
    /// The command line did not make sense. Exits 2, never 1 — a usage
    /// mistake is not a finding.
    Usage(String),
}

impl Error {
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub fn usage(msg: impl Into<String>) -> Self {
        Error::Usage(msg.into())
    }

    /// Name the file a syntax error came from.
    ///
    /// A position is only "a line you can open" once you know which file to
    /// open it in, and `expected a value at 1:1` on a directory of sixteen
    /// lockfiles names none of them. The three parsers work on a string and
    /// have never seen a path; threading one through every `err()` call in
    /// `json.rs`, `yaml.rs` and `toml.rs` would be three files of churn to
    /// print one prefix. Whoever opened the file already knows the name, so
    /// they attach it.
    ///
    /// Only `Syntax` is wrapped. `Io` and `Usage` already lead with their own
    /// path, and a second copy reads like two different files.
    pub fn in_file(self, path: &Path) -> Self {
        match self {
            Error::Syntax { .. } => Error::InFile {
                path: path.display().to_string(),
                source: Box::new(self),
            },
            already_named => already_named,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Syntax { what, line, col } => write!(f, "{what} at {line}:{col}"),
            Error::InFile { path, source } => write!(f, "{path}: {source}"),
            Error::Io { path, source } => write!(f, "{path}: {source}"),
            Error::Usage(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            Error::InFile { source, .. } => Some(&**source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
