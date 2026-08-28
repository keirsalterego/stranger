//! Argument parsing.
//!
//! Flags, a subcommand and three exit codes do not need a parser generator,
//! and a hand-written one gives better errors than a derive macro does: it can
//! say what it expected here rather than printing a grammar.

use crate::error::{Error, Result};
use crate::rules::Severity;
use std::path::PathBuf;

pub const USAGE: &str = "\
stranger — audit a dependency tree without installing, resolving, or phoning anything

usage:
  stranger scan [path]           audit the lockfiles under `path` (default: .)

options:
  --format <human|json>          output format (default: human)
  --fail-on <level>              exit 1 when a finding is at or above
                                 low | medium | high | critical
  --no-color                     never colour output (NO_COLOR, CLICOLOR_FORCE)
  -q, --quiet                    findings only, no summary lines
  -h, --help                     this
  -V, --version                  version

exit codes:
  0  clean, or findings below the --fail-on threshold
  1  a finding at or above the threshold
  2  bad usage, or a file that could not be read
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Auto,
    Never,
}

#[derive(Debug)]
pub enum Command {
    Scan(Options),
    Help,
    Version,
}

#[derive(Debug)]
pub struct Options {
    pub path: PathBuf,
    pub format: Format,
    pub fail_on: Option<Severity>,
    pub color: Color,
    pub quiet: bool,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command> {
    let mut args = args.into_iter().skip(1).peekable();

    let Some(first) = args.next() else {
        return Ok(Command::Help);
    };

    match first.as_str() {
        "-h" | "--help" | "help" => return Ok(Command::Help),
        "-V" | "--version" => return Ok(Command::Version),
        "scan" => {}
        other => {
            return Err(Error::usage(format!(
                "unknown command `{other}`; stranger takes `scan`"
            )));
        }
    }

    let mut opts = Options {
        path: PathBuf::from("."),
        format: Format::Human,
        fail_on: None,
        color: Color::Auto,
        quiet: false,
    };
    let mut saw_path = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                let v = args
                    .next()
                    .ok_or_else(|| Error::usage("--format needs a value"))?;
                opts.format = match v.as_str() {
                    "human" => Format::Human,
                    "json" => Format::Json,
                    other => {
                        return Err(Error::usage(format!(
                            "--format takes `human` or `json`, not `{other}`"
                        )));
                    }
                };
            }
            "--fail-on" => {
                let v = args
                    .next()
                    .ok_or_else(|| Error::usage("--fail-on needs a value"))?;
                opts.fail_on = Some(Severity::parse(&v).ok_or_else(|| {
                    Error::usage(format!(
                        "--fail-on takes low, medium, high or critical, not `{v}`"
                    ))
                })?);
            }
            "--no-color" => opts.color = Color::Never,
            "-q" | "--quiet" => opts.quiet = true,
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with('-') => {
                return Err(Error::usage(format!("unknown option `{other}`")));
            }
            path => {
                if saw_path {
                    return Err(Error::usage(format!(
                        "scan takes one path; got a second, `{path}`"
                    )));
                }
                opts.path = PathBuf::from(path);
                saw_path = true;
            }
        }
    }

    Ok(Command::Scan(opts))
}
