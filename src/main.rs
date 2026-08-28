#![forbid(unsafe_code)]

//! Wiring. Everything interesting is in the library.

use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Instant;

use stranger::cli::{self, Color, Command, Format, Options};
use stranger::error::{Error, Result};
use stranger::lock;
use stranger::report;
use stranger::rules::{Finding, Severity, drift, scripts, slopsquat, trivial};
use stranger::term::Term;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("stranger: {e}");
            // A usage mistake or an unreadable file is not a finding, and a CI
            // gate that cannot tell those apart is a CI gate that gets turned
            // off. Findings are 1; everything broken is 2.
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let opts = match cli::parse(std::env::args())? {
        Command::Help => {
            print!("{}", cli::USAGE);
            return Ok(ExitCode::SUCCESS);
        }
        Command::Version => {
            println!("stranger {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        Command::Scan(o) => o,
    };

    let started = Instant::now();
    let lockfiles = if opts.path.is_file() {
        vec![opts.path.clone()]
    } else if opts.path.is_dir() {
        lock::discover(&opts.path)
    } else {
        return Err(Error::usage(format!(
            "{}: no such file or directory",
            opts.path.display()
        )));
    };

    // Asked once, here. Nothing below this line reads the environment again.
    let term = Term::detect(matches!(opts.color, Color::Never));

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if lockfiles.is_empty() {
        // Their FAQ makes degrading gracefully a condition of the ruling that
        // lets us read these files at all, so this is a requirement and not
        // polish. Say what was looked for, exit clean.
        if !opts.quiet {
            writeln!(out, "\n  no lockfile in {}", opts.path.display()).ok();
            writeln!(out, "  looked for: {}\n", lock::KNOWN.join(", ")).ok();
        }
        return Ok(ExitCode::SUCCESS);
    }

    let mut worst: Option<Severity> = None;
    for path in &lockfiles {
        let tree = lock::read(path)?;
        // Called in `rules::ORDER`, so the JSON array comes out worst-first
        // without a second sort. `pinning` is not wired here yet.
        let mut findings = slopsquat::scan(&tree, slopsquat::Config::default());
        findings.extend(scripts::scan(&tree));
        findings.extend(trivial::scan(&tree));
        findings.extend(drift::scan(&tree));
        worst = worst.max(findings.iter().map(|f| f.severity).max());
        emit(&mut out, &opts, term, &tree, &findings, started.elapsed())?;
    }

    Ok(match (opts.fail_on, worst) {
        (Some(threshold), Some(seen)) if seen >= threshold => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    })
}

fn emit(
    out: &mut impl Write,
    opts: &Options,
    term: Term,
    tree: &lock::Tree,
    findings: &[Finding],
    elapsed: std::time::Duration,
) -> Result<()> {
    let r = match opts.format {
        Format::Human => report::human(out, term, tree, findings, elapsed),
        Format::Json => report::json(out, tree, findings, elapsed),
    };
    r.map_err(|e| Error::io("stdout", e))
}
