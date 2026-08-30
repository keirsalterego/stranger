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
  stranger diff <old> <new>      what changed between two lockfiles, and what
                                 that change introduced

options:
  --format <human|json>          output format (default: human); a directory
                                 scan writes one object per lockfile per line,
                                 so the stream is NDJSON — read it a line at a
                                 time, `jq` does, `json.load` does not. A scan
                                 that could not see everything leads with one
                                 more object, the one with no `source` key
  --fail-on <level>              scan: exit 1 when a finding is at or above
                                 low | medium | high | critical. On `diff` it
                                 gates on what the change *introduced*, not on
                                 the whole tree, so a pull request that adds
                                 nothing passes on a tree `scan` would fail
  --depth <n>                    tree: how deep to print out-edges
                                 (default: 3; 0 for no limit)
  --no-color                     never colour output (NO_COLOR, CLICOLOR_FORCE)
  -v, --verbose                  scan: list every finding, not just the critical ones
  -q, --quiet                    drop the header and the prose; findings only
  -h, --help                     this
  -V, --version                  version

Options take `--flag value` or `--flag=value`. A bare `--` ends the options,
so a path that starts with `-` is still reachable.

exit codes:
  0  clean, or findings below the --fail-on threshold
  1  a finding at or above the threshold
  2  bad usage, a file that could not be read, or a directory that could not
     be opened — an unreadable directory outranks the findings, because a
     scan short by an unknown number of lockfiles cannot answer --fail-on
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
    Diff(DiffOptions),
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

#[derive(Debug)]
pub struct DiffOptions {
    pub old: PathBuf,
    pub new: PathBuf,
    pub format: Format,
    pub fail_on: Option<Severity>,
    pub color: Color,
    pub quiet: bool,
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
        "diff" => diff(args),
        other => Err(Error::usage(format!(
            "unknown command `{other}`; stranger takes `scan`, `tree` or `diff`"
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
    // Everything after a bare `--` is a path, however it is spelled. Without
    // it a directory literally named `-v` is unreachable, and the convention
    // costs one bool.
    let mut only_paths = false;

    while let Some(arg) = args.next() {
        let (flag, inline) = if only_paths {
            (arg.as_str(), None)
        } else {
            split_flag(&arg)
        };
        match flag {
            "--" if !only_paths => only_paths = true,
            "--format" => {
                let v = value(inline, &mut args, "--format")?;
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
                let v = value(inline, &mut args, "--fail-on")?;
                opts.fail_on = Some(Severity::parse(&v).ok_or_else(|| {
                    Error::usage(format!(
                        "--fail-on takes low, medium, high or critical, not `{v}`"
                    ))
                })?);
            }
            "--no-color" => {
                no_value(inline, "--no-color")?;
                opts.color = Color::Never;
            }
            "-v" | "--verbose" => {
                no_value(inline, flag)?;
                opts.verbose = true;
            }
            "-q" | "--quiet" => {
                no_value(inline, flag)?;
                opts.quiet = true;
            }
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with('-') && !only_paths => {
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

/// `--flag=value` split from `--flag value`.
///
/// Both spellings are conventional, and `--format=json` is the first thing a
/// person types. Long options only: `-q=1` is not a spelling anyone uses, and
/// a path is a positional that may legitimately contain `=` — splitting those
/// would make `builds=2/package-lock.json` unscannable.
fn split_flag(arg: &str) -> (&str, Option<&str>) {
    match arg.split_once('=') {
        Some((flag, value)) if flag.starts_with("--") && flag.len() > 2 => (flag, Some(value)),
        _ => (arg, None),
    }
}

/// The value of an option, from `=` or from the next argument.
fn value<I: Iterator<Item = String>>(
    inline: Option<&str>,
    args: &mut I,
    flag: &str,
) -> Result<String> {
    match inline {
        Some(v) => Ok(v.to_string()),
        None => args
            .next()
            .ok_or_else(|| Error::usage(format!("{flag} needs a value"))),
    }
}

/// Refuse `--no-color=please`. A switch that silently ignores a value is a
/// switch that lets somebody believe they turned something off.
fn no_value(inline: Option<&str>, flag: &str) -> Result<()> {
    match inline {
        None => Ok(()),
        Some(v) => Err(Error::usage(format!(
            "{flag} is a switch and takes no value, so `={v}` means nothing"
        ))),
    }
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
    let mut only_paths = false;

    while let Some(arg) = args.next() {
        let (flag, inline) = if only_paths {
            (arg.as_str(), None)
        } else {
            split_flag(&arg)
        };
        match flag {
            "--" if !only_paths => only_paths = true,
            "--format" => {
                let v = value(inline, &mut args, "--format")?;
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
                let v = value(inline, &mut args, "--depth")?;
                opts.depth = v.parse().map_err(|_| {
                    Error::usage(format!(
                        "--depth takes a whole number of levels, or 0 for no limit, not `{v}`"
                    ))
                })?;
            }
            "--no-color" => {
                no_value(inline, "--no-color")?;
                opts.color = Color::Never;
            }
            "-q" | "--quiet" => {
                no_value(inline, flag)?;
                opts.quiet = true;
            }
            "-h" | "--help" => return Ok(Command::Help),
            // Named rather than swept into "unknown option", because both are
            // real flags on the sibling command and "unknown" would send
            // somebody looking for a typo they did not make.
            "--fail-on" | "-v" | "--verbose" => {
                return Err(Error::usage(format!(
                    "`{flag}` is a scan flag; tree reports no findings to gate on"
                )));
            }
            other if other.starts_with('-') && !only_paths => {
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

fn diff<I: Iterator<Item = String>>(mut args: I) -> Result<Command> {
    let mut opts = DiffOptions {
        old: PathBuf::new(),
        new: PathBuf::new(),
        format: Format::Human,
        fail_on: None,
        color: Color::Auto,
        quiet: false,
    };
    let mut positional = 0;
    let mut only_paths = false;

    while let Some(arg) = args.next() {
        let (flag, inline) = if only_paths {
            (arg.as_str(), None)
        } else {
            split_flag(&arg)
        };
        match flag {
            "--" if !only_paths => only_paths = true,
            "--format" => {
                let v = value(inline, &mut args, "--format")?;
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
                let v = value(inline, &mut args, "--fail-on")?;
                opts.fail_on = Some(Severity::parse(&v).ok_or_else(|| {
                    Error::usage(format!(
                        "--fail-on takes low, medium, high or critical, not `{v}`"
                    ))
                })?);
            }
            "--no-color" => {
                no_value(inline, "--no-color")?;
                opts.color = Color::Never;
            }
            "-q" | "--quiet" => {
                no_value(inline, flag)?;
                opts.quiet = true;
            }
            "-h" | "--help" => return Ok(Command::Help),
            // Named, like tree does for the scan-only flags: both are real
            // flags elsewhere and "unknown option" would send somebody hunting
            // for a typo they did not make.
            "--depth" => {
                return Err(Error::usage(
                    "`--depth` is a tree flag; diff walks no edges".to_string(),
                ));
            }
            "-v" | "--verbose" => {
                return Err(Error::usage(
                    "`--verbose` is a scan flag; diff already lists every change".to_string(),
                ));
            }
            other if other.starts_with('-') && !only_paths => {
                return Err(Error::usage(format!("unknown option `{other}`")));
            }
            value => {
                positional += 1;
                match positional {
                    1 => opts.old = PathBuf::from(value),
                    2 => opts.new = PathBuf::from(value),
                    _ => {
                        return Err(Error::usage(format!(
                            "diff takes exactly two lockfiles; got a third argument, `{value}`"
                        )));
                    }
                }
            }
        }
    }

    // Both, or neither means anything. One lockfile has nothing to be
    // different from, and defaulting the second to `.` would silently compare
    // a file against a directory scan.
    if opts.old.as_os_str().is_empty() || opts.new.as_os_str().is_empty() {
        return Err(Error::usage(
            "diff needs two lockfiles: stranger diff <old> <new>",
        ));
    }
    Ok(Command::Diff(opts))
}
