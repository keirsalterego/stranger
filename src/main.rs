#![forbid(unsafe_code)]

//! Wiring. Everything interesting is in the library.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use stranger::cli::{self, Color, Command, Format, Options, TreeOptions};
use stranger::error::{Error, Result};
use stranger::lock;
use stranger::report;
use stranger::rules::{Finding, Severity, drift, pinning, scripts, slopsquat, trivial};
use stranger::term::Term;
use stranger::tree;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        // `stranger tree x | head` closes the pipe as soon as head has what
        // it wants, and every write after that is EPIPE. That is the shell
        // working correctly, not a failure, so it says nothing — the
        // alternative is an error message on every piped invocation.
        //
        // A *scan* never reaches here: `--fail-on` computed an answer before
        // the pipe closed, and returning 0 instead of that answer let
        // `stranger scan --fail-on critical | head -1` report clean on a tree
        // with criticals in it. `run` swallows the pipe itself and keeps the
        // code. This arm covers the paths that have no answer to lose.
        Err(e) if broken_pipe(&e) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("stranger: {e}");
            // A usage mistake or an unreadable file is not a finding, and a CI
            // gate that cannot tell those apart is a CI gate that gets turned
            // off. Findings are 1; everything broken is 2.
            ExitCode::from(2)
        }
    }
}

/// True for the one I/O error that means "nobody is reading any more".
fn broken_pipe(e: &Error) -> bool {
    matches!(e, Error::Io { source, .. } if source.kind() == io::ErrorKind::BrokenPipe)
}

/// argv as `String`s, or a usage error naming the argument that was not UTF-8.
///
/// `std::env::args()` *panics* on a non-UTF-8 argument, and on Linux argv is
/// arbitrary bytes: `stranger scan $'/tmp/\xff'` aborted the process at
/// `library/std/src/env.rs` before a line of this crate ran, exit 134. A tool
/// whose whole argument is that it degrades gracefully cannot abort on a
/// filename. Paths that are not UTF-8 are refused rather than scanned, which
/// is a real limit and is named in README LIMITS.
fn args() -> Result<Vec<String>> {
    std::env::args_os()
        .map(|a| {
            a.into_string().map_err(|bad| {
                Error::usage(format!(
                    "argument is not valid UTF-8: {}",
                    bad.to_string_lossy()
                ))
            })
        })
        .collect()
}

fn run() -> Result<ExitCode> {
    let opts = match cli::parse(args()?)? {
        Command::Help => {
            print!("{}", cli::USAGE);
            return Ok(ExitCode::SUCCESS);
        }
        Command::Version => {
            println!("stranger {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        Command::Scan(o) => o,
        Command::Tree(o) => return tree(o),
    };

    let lockfiles = lockfiles(&opts.path)?;

    // Asked once, here. Nothing below this line reads the environment again.
    let term = Term::detect(matches!(opts.color, Color::Never));

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if lockfiles.is_empty() {
        // Their FAQ makes degrading gracefully a condition of the ruling that
        // lets us read these files at all, so this is a requirement and not
        // polish. Say what was looked for, exit clean.
        //
        // Prose only in human mode. `--format json` is one object per lockfile
        // on its own line, so the honest JSON answer to no lockfiles is no
        // lines — and it is the answer a consumer reading the stream a line at
        // a time already handles. Printing the sentence here was the one path
        // in the tool that wrote something other than JSON to a JSON stream.
        if !opts.quiet && matches!(opts.format, Format::Human) {
            nothing_to_read(&mut out, &opts.path).ok();
        }
        return Ok(ExitCode::SUCCESS);
    }

    let mut worst: Option<Severity> = None;
    let mut read = 0usize;
    // Set when stdout goes away mid-report. Everything after it is computed
    // and not printed, because the exit code is the half of a CI gate's
    // output that survives `| head`.
    let mut hung_up = false;

    // In path order, results and failures alike — `scan_all` hands them back
    // in the order the paths came in, so the stderr stream is as reproducible
    // as the stdout one. Reporting whichever thread failed first was the one
    // part of a scan that changed between two runs over the same tree.
    for scanned in scan_all(&lockfiles) {
        match scanned {
            Ok(a) => {
                read += 1;
                worst = worst.max(a.findings.iter().map(|f| f.severity).max());
                if !hung_up {
                    match emit(&mut out, &opts, term, &a.tree, &a.findings, a.elapsed) {
                        Ok(()) => {}
                        Err(e) if broken_pipe(&e) => hung_up = true,
                        Err(e) => return Err(e),
                    }
                }
            }
            // One corrupt lockfile is one lockfile's problem. Throwing away
            // fifteen siblings' findings because the sixteenth is garbage is
            // the failure the graceful-degradation requirement above is about,
            // and it is worst exactly when it matters most: the tree somebody
            // half-generated is the tree most likely to hold both.
            //
            // stderr, so `--format json` stays parseable while the complaint
            // still reaches a person.
            Err(e) => eprintln!("stranger: {e}"),
        }
    }

    // Exit 2 is "stranger could not do its job", not "stranger hit a bump". A
    // scan that read something reported findings, so those decide the code as
    // usual and the unreadable file is on stderr where a person will see it; a
    // scan that read nothing has nothing to say, and letting that exit 0 would
    // have a CI gate call an unopenable tree clean. The middle case — some
    // read, some not — is deliberately not its own code: `--fail-on` answers
    // "is there a finding", and no third value fits in that answer.
    if read == 0 {
        return Ok(ExitCode::from(2));
    }

    Ok(match (opts.fail_on, worst) {
        (Some(threshold), Some(seen)) if seen >= threshold => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    })
}

fn lockfiles(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        Ok(vec![path.to_path_buf()])
    } else if path.is_dir() {
        Ok(lock::discover(path))
    } else {
        // `symlink_metadata`, so the three cases are told apart. `is_file() ||
        // is_dir()` being false does not mean the path is absent: /dev/null, a
        // FIFO and a symlink pointing at nothing all fail both and all three
        // exist. Saying "no such file or directory" about a device node sends
        // somebody looking for a typo that is not there.
        let what = match std::fs::symlink_metadata(path) {
            Ok(_) => "not a regular file or directory",
            Err(e) if e.kind() == io::ErrorKind::NotFound => "no such file or directory",
            Err(_) => "cannot be read",
        };
        Err(Error::usage(format!("{}: {what}", path.display())))
    }
}

fn nothing_to_read(out: &mut impl Write, path: &Path) -> io::Result<()> {
    writeln!(out, "\n  no lockfile in {}", path.display())?;
    writeln!(out, "  looked for: {}\n", lock::KNOWN.join(", "))
}

/// `stranger tree <pkg>` — read the same lockfiles, run no rules, and print the
/// graph around one name.
///
/// Sequential where `scan` is threaded, and that is not an oversight: the
/// expensive half of a scan is the nearest-neighbour sweep over a 140,066-name
/// corpus, which is exactly the part this does not do. Reading sixteen fixtures
/// takes long enough to notice and not long enough to thread, and reading them
/// in path order means the output is in path order for free.
fn tree(opts: TreeOptions) -> Result<ExitCode> {
    let paths = lockfiles(&opts.path)?;
    let term = Term::detect(matches!(opts.color, Color::Never));
    let stdout = io::stdout();
    let mut out = stdout.lock();

    if paths.is_empty() && matches!(opts.format, Format::Human) {
        // Same degradation as a scan of an empty directory: say what was
        // looked for, exit clean. A JSON consumer gets the `found: false`
        // object below instead, which already carries `lockfiles: 0`.
        nothing_to_read(&mut out, &opts.path).map_err(|e| Error::io("stdout", e))?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut trees = Vec::with_capacity(paths.len());
    let mut failed = 0usize;
    for path in &paths {
        match lock::read(path) {
            Ok(t) => trees.push(t),
            // One corrupt lockfile is one lockfile's problem, exactly as in a
            // scan: the name you are asking about may well be in a sibling.
            Err(e) => {
                eprintln!("stranger: {e}");
                failed += 1;
            }
        }
    }
    if trees.is_empty() && failed > 0 {
        return Ok(ExitCode::from(2));
    }

    let report = tree::Report::build(&trees, &opts.package, &opts.path, opts.depth);
    match opts.format {
        Format::Human => tree::human(&mut out, term, &report, opts.quiet),
        Format::Json => tree::json(&mut out, &report),
    }
    .map_err(|e| Error::io("stdout", e))?;

    // A package that is not there is an answer, not a failure. Exit 0 and list
    // what is close, because "no such package" plus a stack trace is how a tool
    // teaches people not to run it.
    Ok(ExitCode::SUCCESS)
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
/// That holds for the failures too, which is why a file that could not be read
/// comes back as its own slot's `Err` rather than as this function's: hoisting
/// the first one out lost every sibling's findings *and* made the surfaced
/// error a race between threads.
///
/// ponytail: one thread per lockfile, not a pool. A repo with four hundred
/// lockfiles would spawn four hundred threads, and the fix then is to chunk the
/// slice across `available_parallelism()` — but the walk skips `node_modules`,
/// so the realistic count is single digits and a pool would be scaffolding for
/// a case that does not arrive.
fn scan_all(lockfiles: &[PathBuf]) -> Vec<Result<Audit>> {
    if lockfiles.len() == 1 {
        return vec![audit(&lockfiles[0])];
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

    let mut done: Vec<Option<Result<Audit>>> = (0..lockfiles.len()).map(|_| None).collect();
    for (i, result) in rx {
        done[i] = Some(result);
    }
    // Every slot is filled: the scope joined every thread before returning, and
    // each one sent exactly once.
    done.into_iter()
        .map(|d| d.expect("every lockfile reported"))
        .collect()
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
    let mut findings = slopsquat::scan(&tree, slopsquat::Config::default());
    findings.extend(scripts::scan(&tree));
    findings.extend(trivial::scan(&tree));
    findings.extend(drift::scan(&tree));
    findings.extend(pinning::scan(&tree));
    // Report order, once, here — not "the order the calls happen to be in",
    // which is what a comment on this line used to claim. `report::human`
    // sorts its own blocks by `rank`; the JSON array did not, so the two
    // surfaces disagreed about order on any tree where a later rule outranked
    // an earlier one. Stable, so the within-rule order each rule chose (the
    // slopsquat rule sorts by name) survives.
    findings.sort_by_key(|f| f.rule.rank());
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
        Format::Json => report::json(out, tree, findings),
    };
    r.map_err(|e| Error::io("stdout", e))
}
