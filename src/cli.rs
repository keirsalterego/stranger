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
  stranger tree <pkg> [path]     what depends on `pkg`, and what `pkg` depends on

options:
  --format <human|json>          output format (default: human)
  --fail-on <level>              scan: exit 1 when a finding is at or above
                                 low | medium | high | critical
  --depth <n>                    tree: how deep to print out-edges
                                 (default: 3; 0 for no limit)
  --no-color                     never colour output (NO_COLOR, CLICOLOR_FORCE)
  -v, --verbose                  scan: list every finding, not just the critical ones
  -q, --quiet                    drop the header and the prose; findings only
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
    Tree(TreeOptions),
    Help,
    Version,
}

#[derive(Debug)]
pub struct Options {
    pub path: PathBuf,
    pub verbose: bool,
    pub format: Format,
    pub fail_on: Option<Severity>,
    pub color: Color,
    pub quiet: bool,
}

#[derive(Debug)]
pub struct TreeOptions {
    pub package: String,
    pub path: PathBuf,
    pub format: Format,
    pub color: Color,
    pub quiet: bool,
    /// 0 means no limit. See `tree::DEFAULT_DEPTH`.
    pub depth: usize,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command> {
    let mut args = args.into_iter().skip(1);

    let Some(first) = args.next() else {
        return Ok(Command::Help);
    };

    match first.as_str() {
        "-h" | "--help" | "help" => Ok(Command::Help),
        "-V" | "--version" => Ok(Command::Version),
        "scan" => scan(args),
        "tree" => tree(args),
        other => Err(Error::usage(format!(
            "unknown command `{other}`; stranger takes `scan` or `tree`"
        ))),
    }
}

fn scan<I: Iterator<Item = String>>(mut args: I) -> Result<Command> {
    let mut opts = Options {
        path: PathBuf::from("."),
        format: Format::Human,
        fail_on: None,
        color: Color::Auto,
        quiet: false,
        verbose: false,
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
            "-v" | "--verbose" => opts.verbose = true,
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

fn tree<I: Iterator<Item = String>>(mut args: I) -> Result<Command> {
    let mut opts = TreeOptions {
        package: String::new(),
        path: PathBuf::from("."),
        format: Format::Human,
        color: Color::Auto,
        quiet: false,
        depth: crate::tree::DEFAULT_DEPTH,
    };
    let mut positional = 0;

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
            "--depth" => {
                let v = args
                    .next()
                    .ok_or_else(|| Error::usage("--depth needs a value"))?;
                opts.depth = v.parse().map_err(|_| {
                    Error::usage(format!(
                        "--depth takes a whole number of levels, or 0 for no limit, not `{v}`"
                    ))
                })?;
            }
            "--no-color" => opts.color = Color::Never,
            "-q" | "--quiet" => opts.quiet = true,
            "-h" | "--help" => return Ok(Command::Help),
            // Named rather than swept into "unknown option", because both are
            // real flags on the sibling command and "unknown" would send
            // somebody looking for a typo they did not make.
            "--fail-on" | "-v" | "--verbose" => {
                return Err(Error::usage(format!(
                    "`{arg}` is a scan flag; tree reports no findings to gate on"
                )));
            }
            other if other.starts_with('-') => {
                return Err(Error::usage(format!("unknown option `{other}`")));
            }
            value => {
                positional += 1;
                match positional {
                    1 => opts.package = value.to_string(),
                    2 => opts.path = PathBuf::from(value),
                    _ => {
                        return Err(Error::usage(format!(
                            "tree takes a package and one path; got a third argument, `{value}`"
                        )));
                    }
                }
            }
        }
    }

    if opts.package.is_empty() {
        return Err(Error::usage(
            "tree needs a package name: stranger tree <pkg> [path]",
        ));
    }
    Ok(Command::Tree(opts))
}
