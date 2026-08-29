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
use stranger::term::{self, Term};
use stranger::tree;
use stranger::walk::Walk;

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

    let walk = discover(&opts.path)?;

    // Asked once, here. Nothing below this line reads the environment again.
    let term = Term::detect(matches!(opts.color, Color::Never));

    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Before the reports, because it changes how every line below it should be
    // read, and because `| head -5` must not be able to hide it.
    //
    // `.ok()`: a caveat that could not be printed must not change the exit
    // code. The one it qualifies is decided below from `walk` directly, so the
    // failure mode of a closed stdout here is a missing sentence and a correct
    // 2, rather than an error about stdout standing in for the answer.
    blind_spots(
        &mut out,
        &walk,
        &opts.path,
        opts.format,
        opts.verbose,
        opts.quiet,
    )
    .ok();

    if walk.found.is_empty() && walk.unreadable.is_empty() {
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
    for scanned in scan_all(&walk.found) {
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

    // Exit 2 is "stranger could not do its job", not "stranger hit a bump".
    //
    // `read == 0` is the older half: lockfiles were found and none of them
    // parsed, so there is nothing to gate on and exiting 0 would have a CI
    // gate call an unreadable tree clean. A scan that read *something*
    // reported findings, and those decide the code as usual while the file
    // that would not parse sits on stderr where a person will see it. The
    // middle case is deliberately not its own code: `--fail-on` answers "is
    // there a finding", and no third value fits in that answer.
    //
    // A directory the walk could not open is the other half, and it outranks
    // the findings even when its siblings read fine. An unreadable *root* is
    // the easy half of that — stranger was pointed at something it cannot
    // open, and there is no answer at all. The judgement call is the
    // unreadable *subdirectory*, which looks like the unparseable-sibling case
    // above and is not, because the two holes are different shapes. A lockfile
    // that will not parse is named: the list of what is there is complete,
    // one entry of it failed, and the reader can see exactly what they are
    // missing. A directory that will not open removes an unknown number of
    // entries from that list, and stranger cannot say whether it held nothing
    // or held the poisoned file. `--fail-on` asks "is there a finding at or
    // above this level" over a list that is short by an unknown amount, and
    // the honest answer to that is neither 0 nor 1.
    //
    // The cost is real and I am choosing to pay it: one 0700 directory
    // anywhere under the scan root turns the gate red, and the fix is the
    // user's to make without a flag from us — chmod it, or point stranger at
    // the subtrees it can read. The alternative is a green tick over a
    // directory nobody could open, which is the exact failure this tool exists
    // to find in other people's dependency trees.
    if !walk.unreadable.is_empty() || read == 0 {
        return Ok(ExitCode::from(2));
    }

    Ok(match (opts.fail_on, worst) {
        (Some(threshold), Some(seen)) if seen >= threshold => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    })
}

fn discover(path: &Path) -> Result<Walk> {
    if path.is_file() {
        // A path given by hand is not discovery. Nothing was skipped and
        // nothing could have gone unlooked-at, so the three blind-spot lists
        // are empty by construction rather than by luck.
        Ok(Walk {
            found: vec![path.to_path_buf()],
            ..Walk::default()
        })
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

/// "I looked and found nothing", said so it cannot be mistaken for "I could not
/// look" — which is what `blind_spots` above it has already ruled out.
fn nothing_to_read(out: &mut impl Write, path: &Path) -> io::Result<()> {
    writeln!(out, "\n  no lockfile stranger reads in {}", path.display())?;
    writeln!(out, "  looked for: {}\n", lock::KNOWN.join(", "))
}

/// Everything the walk did not look at, printed before the report it qualifies.
///
/// Three lists, three audiences. The unreadable paths are a failure and go out
/// whatever the flags say, because they are about to become an exit code and a
/// report that does not mention them is a report that lies by omission. The
/// unsupported lockfiles are the FAQ's graceful-degradation condition: naming
/// `yarn.lock` is not reading it, but "no lockfile in ." told somebody holding
/// one that their repository has none, and a declared cut the user can see is
/// a different thing from a cut only the author knows about. The skipped
/// directories are policy — thirteen names, every hidden directory and
/// everything past `walk::MAX_DEPTH` — so they stay behind `-v`, but policy
/// that hides a lockfile hides a lockfile just the same.
///
/// In JSON mode all of it collapses to one object on one line. Prose on that
/// stream would be the one thing that breaks a consumer reading it line by
/// line; a second object shape does not, and it has no `source` key, which is
/// how a consumer tells it from a scan. `--quiet` keeps the failure and drops
/// the rest.
fn blind_spots(
    out: &mut impl Write,
    walk: &Walk,
    root: &Path,
    format: Format,
    verbose: bool,
    quiet: bool,
) -> io::Result<()> {
    let unsupported = if quiet { &[][..] } else { &walk.unsupported };
    if walk.unreadable.is_empty() && unsupported.is_empty() {
        return skipped(out, walk, root, format, verbose, quiet);
    }

    if matches!(format, Format::Json) {
        write!(out, "{{\"unreadable\":[")?;
        for (i, p) in walk.unreadable.iter().enumerate() {
            if i > 0 {
                write!(out, ",")?;
            }
            report::string(out, &term::sanitize(&p.display().to_string()))?;
        }
        write!(out, "],\"unsupported\":[")?;
        for (i, name) in unsupported.iter().enumerate() {
            if i > 0 {
                write!(out, ",")?;
            }
            report::string(out, name)?;
        }
        return writeln!(out, "]}}");
    }

    if !walk.unreadable.is_empty() {
        let n = walk.unreadable.len();
        let s = if n == 1 { "" } else { "s" };
        writeln!(
            out,
            "\n  could not look inside {n} path{s} — this scan is incomplete"
        )?;
        for p in &walk.unreadable {
            writeln!(out, "     {}", term::sanitize(&p.display().to_string()))?;
        }
        // The blank line belongs to whatever comes next, and everything that
        // can come next brings its own.
        if unsupported.is_empty() {
            writeln!(out)?;
        }
    }
    if !unsupported.is_empty() {
        writeln!(out, "\n  found but not read: {}", unsupported.join(", "))?;
    }
    skipped(out, walk, root, format, verbose, quiet)
}

/// The `-v` half. Split out only because `blind_spots` has two early returns
/// that both still owe it.
fn skipped(
    out: &mut impl Write,
    walk: &Walk,
    root: &Path,
    format: Format,
    verbose: bool,
    quiet: bool,
) -> io::Result<()> {
    if !verbose || quiet || walk.skipped.is_empty() || matches!(format, Format::Json) {
        return Ok(());
    }
    // Relative to the scan root, because the absolute paths are all the same
    // for the first sixty characters and the part that differs is the part
    // being reported.
    let names: Vec<String> = walk
        .skipped
        .iter()
        .map(|(p, _)| {
            term::sanitize(&p.strip_prefix(root).unwrap_or(p).display().to_string()).into_owned()
        })
        .collect();
    let w = term::column(names.iter().map(String::as_str), 24);
    writeln!(out, "\n  not descended into ({})", walk.skipped.len())?;
    for (name, (_, why)) in names.iter().zip(&walk.skipped) {
        writeln!(out, "     {} {why}", term::pad(name, w))?;
    }
    Ok(())
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
    let walk = discover(&opts.path)?;
    let term = Term::detect(matches!(opts.color, Color::Never));
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // `tree` has no `-v`, so the skipped list stays unprinted here — the rest
    // matters just as much as it does in a scan. "no such package" over a
    // directory that would not open is the same wrong answer as "no findings".
    blind_spots(&mut out, &walk, &opts.path, opts.format, false, opts.quiet).ok();
    if !walk.unreadable.is_empty() {
        return Ok(ExitCode::from(2));
    }

    let paths = walk.found;
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
/// The degradation promise stops at `Err`, and the doc comments elsewhere in
/// this file state it without that qualification. A worker that *panics* still
/// takes the whole scan with it: `thread::scope` re-raises a panicking child
/// when it joins, and the release profile sets `panic = "abort"`, so there is
/// no unwinding left to catch even if this wanted to. Every parser is written
/// to return `Error` rather than panic and the fuzz suite in `tests/fuzz.rs`
/// is what keeps that true, which is a smaller guarantee than "one file's
/// problem stays one file's problem" and is the one actually on offer.
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
