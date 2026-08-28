#![forbid(unsafe_code)]

//! Wiring. Everything interesting is in the library.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use stranger::cli::{self, Color, Command, Format, Options};
use stranger::error::{Error, Result};
use stranger::lock;
use stranger::report;
use stranger::rules::{Finding, Severity, drift, pinning, scripts, slopsquat, trivial};
use stranger::term::Term;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        // `stranger scan . | head` closes the pipe as soon as head has what it
        // wants, and every write after that is EPIPE. That is the shell
        // working correctly, not a failure, so it exits 0 and says nothing —
        // the alternative is an error message on every piped invocation.
        Err(Error::Io { source, .. }) if source.kind() == io::ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
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

    let scanned = scan_all(&lockfiles)?;

    let mut worst: Option<Severity> = None;
    for a in &scanned {
        worst = worst.max(a.findings.iter().map(|f| f.severity).max());
        emit(&mut out, &opts, term, &a.tree, &a.findings, a.elapsed)?;
    }

    Ok(match (opts.fail_on, worst) {
        (Some(threshold), Some(seen)) if seen >= threshold => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    })
}

/// Read and audit every lockfile, one thread each, and hand back the results in
/// the order the paths came in.
///
/// A monorepo scan is several independent files and the slow part of each is the
/// corpus search, which is pure CPU over a shared read-only slice. `std::thread`
/// with a scope covers it: the closures borrow `lockfiles` rather than cloning
/// paths into each thread, and the channel collects results as they finish.
///
/// Output order is the input order, not the finishing order. Two runs over the
/// same tree have to produce the same bytes or a diff between scans is noise.
///
/// ponytail: one thread per lockfile, not a pool. A repo with four hundred
/// lockfiles would spawn four hundred threads, and the fix then is to chunk the
/// slice across `available_parallelism()` — but the walk skips `node_modules`,
/// so the realistic count is single digits and a pool would be scaffolding for
/// a case that does not arrive.
fn scan_all(lockfiles: &[PathBuf]) -> Result<Vec<Audit>> {
    if lockfiles.len() == 1 {
        return Ok(vec![audit(&lockfiles[0])?]);
    }

    let (tx, rx) = mpsc::channel();
    thread::scope(|s| {
        for (i, path) in lockfiles.iter().enumerate() {
            let tx = tx.clone();
            s.spawn(move || {
                // `SendError` hands the whole payload back, so returning this
                // from the closure makes the thread's return type enormous.
                // The receiver outlives the scope and cannot have hung up, so
                // there is nothing to handle either way.
                let _ = tx.send((i, audit(path)));
            });
        }
    });
    drop(tx);

    let mut done: Vec<Option<Audit>> = (0..lockfiles.len()).map(|_| None).collect();
    for (i, result) in rx {
        done[i] = Some(result?);
    }
    // Every slot is filled: the scope joined every thread before returning, and
    // each one sent exactly once.
    Ok(done
        .into_iter()
        .map(|d| d.expect("every lockfile reported"))
        .collect())
}

/// One lockfile's result, with the time *that file* took.
///
/// Not the time since the process started. Reporting the running total against
/// each file makes the last one in a directory scan look like the slow one.
struct Audit {
    tree: lock::Tree,
    findings: Vec<Finding>,
    elapsed: std::time::Duration,
}

fn audit(path: &Path) -> Result<Audit> {
    let started = Instant::now();
    let tree = lock::read(path)?;
    // Called in `rules::ORDER`, so the JSON array comes out worst-first without
    // a second sort.
    let mut findings = slopsquat::scan(&tree, slopsquat::Config::default());
    findings.extend(scripts::scan(&tree));
    findings.extend(trivial::scan(&tree));
    findings.extend(drift::scan(&tree));
    findings.extend(pinning::scan(&tree));
    Ok(Audit {
        tree,
        findings,
        elapsed: started.elapsed(),
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
        Format::Human => {
            report::human(out, term, tree, findings, elapsed, opts.verbose, opts.quiet)
        }
        Format::Json => report::json(out, tree, findings, elapsed),
    };
    r.map_err(|e| Error::io("stdout", e))
}
