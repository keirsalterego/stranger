//! The command line as a user meets it: exit codes, formats, and the promise
//! that a missing lockfile is not a crash.
//!
//! These drive the built binary rather than the library, because exit codes and
//! stdout are the actual contract with a CI job and neither is visible from
//! inside `main`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    // The test binary lives in target/<profile>/deps/, so the CLI is two up.
    let mut p = std::env::current_exe().expect("test binary has a path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("stranger")
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("stranger should run")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A scratch directory under `CARGO_TARGET_TMPDIR`, emptied first — these
/// tests assert on how many lockfiles were found, so a leftover file from a
/// previous run is a false pass waiting to happen.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir");
    }
    std::fs::write(path, body).expect("write");
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name),
    )
    .expect("fixture")
}

const POISONED: &str = "fixtures/poisoned.package-lock.json";
const CLEAN: &str = "fixtures/npm-xs.package-lock.json";

#[test]
fn clean_scan_exits_zero() {
    let o = run(&["scan", CLEAN]);
    assert_eq!(o.status.code(), Some(0));
}

/// Findings alone are not a failure. Only crossing the threshold you asked for
/// is, because a gate that fails on everything gets turned off.
#[test]
fn findings_without_a_threshold_still_exit_zero() {
    let o = run(&["scan", POISONED]);
    assert!(stdout(&o).contains("HALLUCINATION RISK"));
    assert_eq!(o.status.code(), Some(0));
}

#[test]
fn fail_on_gates_by_severity() {
    assert_eq!(
        run(&["scan", POISONED, "--fail-on", "critical"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        run(&["scan", POISONED, "--fail-on", "low"]).status.code(),
        Some(1)
    );
    // Nothing in a clean tree reaches the threshold.
    assert_eq!(
        run(&["scan", CLEAN, "--fail-on", "critical"]).status.code(),
        Some(0)
    );
}

/// Usage mistakes and unreadable files are 2, never 1. A CI job that cannot
/// tell "I found something" from "I am broken" reports the wrong thing.
#[test]
fn broken_things_exit_two() {
    assert_eq!(run(&["scan", "no/such/path"]).status.code(), Some(2));
    assert_eq!(run(&["--nonsense"]).status.code(), Some(2));
    assert_eq!(
        run(&["scan", POISONED, "--format", "xml"]).status.code(),
        Some(2)
    );
    assert_eq!(
        run(&["scan", POISONED, "--fail-on", "urgent"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(run(&["scan", "Cargo.toml"]).status.code(), Some(2));
}

/// The hackathon FAQ makes graceful degradation a condition of the ruling that
/// lets this tool read lockfiles at all, so it is a requirement, not polish.
#[test]
fn a_directory_with_no_lockfile_says_so_and_exits_zero() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli_empty");
    std::fs::create_dir_all(&dir).unwrap();
    let o = run(&["scan", dir.to_str().unwrap()]);
    let out = stdout(&o);
    assert!(out.contains("no lockfile"), "{out}");
    assert!(out.contains("looked for"), "{out}");
    assert_eq!(o.status.code(), Some(0));
}

/// One corrupt lockfile used to cost the whole tree: the first `Err` came out
/// of the thread pool and every sibling's findings went with it, which is the
/// worst possible time for it — a half-generated tree is exactly the tree
/// holding both a garbage file and the hallucinations you were scanning for.
#[test]
fn one_unreadable_lockfile_does_not_cancel_its_siblings() {
    let dir = scratch("cli_mixed");
    write(
        &dir.join("good/package-lock.json"),
        &fixture("poisoned.package-lock.json"),
    );
    write(&dir.join("bad/package-lock.json"), "garbage{");

    let o = run(&["scan", dir.to_str().unwrap()]);
    let out = stdout(&o);
    for planted in ["chalck@5.3.0", "expres@4.18.2", "lodahs@4.17.21"] {
        assert!(out.contains(planted), "{planted} missing from:\n{out}");
    }
    // The complaint reaches a person, on the stream that is not the report.
    let err = stderr(&o);
    assert!(err.contains("bad"), "{err}");
    assert!(
        err.contains("1:1"),
        "the position survives the wrapping: {err}"
    );
    // Findings printed, so the findings decide the code. Nothing was broken
    // enough to stop the scan doing its job.
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert_eq!(
        run(&["scan", dir.to_str().unwrap(), "--fail-on", "critical"])
            .status
            .code(),
        Some(1),
        "a surviving critical still gates"
    );
}

/// The other side of it: 2 is for a scan that could not do its job at all, and
/// a tree where nothing at all could be read is that scan. Exiting 0 there
/// would have a gate call an unopenable tree clean.
#[test]
fn a_tree_where_nothing_reads_exits_two() {
    let dir = scratch("cli_all_bad");
    write(&dir.join("a/package-lock.json"), "garbage{");
    write(&dir.join("b/package-lock.json"), "{\"lockfileVersion\"");

    let o = run(&["scan", dir.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert_eq!(stdout(&o), "", "nothing was read, so nothing is reported");
}

/// Both failures name their own file, in path order, every time. The reported
/// error used to be whichever thread finished first — six runs over the same
/// two files gave two different messages, on the one code path whose whole
/// promise is that two runs produce the same bytes.
#[test]
fn the_error_stream_is_in_path_order_and_deterministic() {
    let dir = scratch("cli_two_bad");
    write(&dir.join("a/package-lock.json"), "garbage{");
    write(&dir.join("b/package-lock.json"), "{\"lockfileVersion\"");

    let once = stderr(&run(&["scan", dir.to_str().unwrap()]));
    let lines: Vec<&str> = once.lines().collect();
    assert_eq!(lines.len(), 2, "{once}");
    assert!(lines[0].contains("a"), "{once}");
    assert!(lines[1].contains("b"), "{once}");
    for _ in 0..5 {
        assert_eq!(stderr(&run(&["scan", dir.to_str().unwrap()])), once);
    }
}

/// A syntax error is only "a line you can open" if it says which file. On a
/// directory of sixteen lockfiles `expected a value at 1:1` names none of
/// them, and the parsers cannot say — they are handed a string.
#[test]
fn a_syntax_error_names_its_file() {
    let dir = scratch("cli_named");
    let path = dir.join("package-lock.json");
    write(&path, "garbage{");
    let err = stderr(&run(&["scan", path.to_str().unwrap()]));
    assert!(err.contains("package-lock.json"), "{err}");
    assert!(err.contains("at 1:1"), "{err}");
}

/// JSON mode is one object per lockfile on its own line, so no lockfiles is no
/// lines. The prose was written to stdout regardless of `--format`, which made
/// this the one way to get something other than JSON out of a JSON stream.
#[test]
fn json_on_a_directory_with_no_lockfile_stays_json() {
    let dir = scratch("cli_empty_json");
    let o = run(&["scan", "--format", "json", dir.to_str().unwrap()]);
    assert_eq!(stdout(&o), "", "{}", stdout(&o));
    assert_eq!(o.status.code(), Some(0));
    // Human mode still says what it looked for. Degrading gracefully is a
    // requirement; it just has to speak the format it was asked for.
    let human = stdout(&run(&["scan", dir.to_str().unwrap()]));
    assert!(human.contains("no lockfile"), "{human}");
}

/// Every line of a JSON scan parses on its own, whatever the tree holds — an
/// empty directory, or several files including one that will not read.
#[test]
fn every_json_line_parses() {
    let dir = scratch("cli_json_lines");
    write(
        &dir.join("good/package-lock.json"),
        &fixture("npm-xs.package-lock.json"),
    );
    write(&dir.join("also/requirements.txt"), "requests==2.31.0\n");
    write(&dir.join("bad/package-lock.json"), "garbage{");
    let empty = scratch("cli_json_lines_empty");

    for target in [empty, dir] {
        let out = stdout(&run(&[
            "scan",
            "--format",
            "json",
            target.to_str().unwrap(),
        ]));
        for line in out.lines() {
            stranger::json::parse(line).unwrap_or_else(|e| panic!("{line}: {e}"));
        }
    }
}

#[test]
fn quiet_drops_the_header_and_the_risk_line() {
    let out = stdout(&run(&["scan", POISONED, "-q"]));
    assert!(
        out.contains("HALLUCINATION RISK"),
        "findings survive: {out}"
    );
    assert!(!out.contains("packages   ("), "header is gone: {out}");
    assert!(!out.contains("risk "), "risk line is gone: {out}");
}

/// Non-critical rules collapse to a count until asked.
#[test]
fn verbose_expands_the_collapsed_rules() {
    let quiet = stdout(&run(&["scan", POISONED]));
    let loud = stdout(&run(&["scan", POISONED, "-v"]));
    assert!(
        !quiet.contains("VERSION DRIFT\n"),
        "drift is a count by default"
    );
    assert!(loud.lines().count() > quiet.lines().count() + 50);
    assert!(loud.contains("2 versions:"));
}

#[test]
fn json_is_parseable_by_our_own_parser() {
    let out = stdout(&run(&["scan", POISONED, "--format", "json"]));
    let v = stranger::json::parse(&out).expect("our JSON output should parse");
    assert_eq!(v.get("ecosystem").and_then(|e| e.as_str()), Some("npm"));
    let findings = v
        .get("findings")
        .and_then(|f| f.as_array())
        .expect("findings array");
    assert!(
        findings
            .iter()
            .any(|f| f.get("rule").and_then(|r| r.as_str()) == Some("slopsquat"))
    );
}

/// Colour is for terminals. A pipe gets bytes you can grep.
#[test]
fn piped_output_carries_no_escape_codes() {
    for args in [
        &["scan", POISONED][..],
        &["scan", POISONED, "--format", "json"][..],
    ] {
        assert!(!stdout(&run(args)).contains('\u{1b}'), "{args:?}");
    }
}

#[test]
fn help_and_version() {
    assert!(stdout(&run(&["--help"])).contains("usage:"));
    assert!(stdout(&run(&["--version"])).contains(env!("CARGO_PKG_VERSION")));
    // No arguments is help, not an error.
    assert_eq!(run(&[]).status.code(), Some(0));
}

/// A directory scan walks, finds several lockfiles across several ecosystems,
/// and audits them on separate threads. Output order is the sorted path order
/// rather than whichever thread finished first — two runs over one tree have
/// to produce the same bytes or a diff between scans is noise.
#[test]
fn a_directory_scan_is_deterministic() {
    let strip_timing = |s: String| {
        s.lines()
            .map(|l| {
                if l.contains("risk ") {
                    "risk".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let once = strip_timing(stdout(&run(&["scan", "fixtures"])));
    for _ in 0..4 {
        assert_eq!(strip_timing(stdout(&run(&["scan", "fixtures"]))), once);
    }
    // Every format, in sorted order.
    let order: Vec<&str> = once
        .lines()
        .filter(|l| l.contains(" packages   ("))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert!(order.len() >= 15, "{order:?}");
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(order, sorted, "results come out in path order");
    assert!(order.iter().any(|f| f.ends_with("Cargo.lock")));
    assert!(order.iter().any(|f| f.ends_with("uv.lock")));
    assert!(order.iter().any(|f| f.ends_with("requirements.txt")));
}

/// The walk must not wander into a vendored `node_modules` and audit four
/// hundred lockfiles belonging to other people.
#[test]
fn a_directory_scan_skips_vendored_lockfiles() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli_vendored");
    let nested = root.join("node_modules/some-dep");
    std::fs::create_dir_all(&nested).unwrap();
    let real = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/npm-xs.package-lock.json"),
    )
    .unwrap();
    std::fs::write(root.join("package-lock.json"), &real).unwrap();
    std::fs::write(nested.join("package-lock.json"), &real).unwrap();

    let out = stdout(&run(&["scan", root.to_str().unwrap()]));
    assert_eq!(out.matches(" packages   (").count(), 1, "{out}");
}

/// The trivial rule skips first-party packages, so its percentage has to be
/// taken against the same population — the count the header prints. These were
/// two different denominators for a while (`packages.len()` against
/// `third_party()`), which on npm-m read 2.9% where the header's own numbers
/// say 3.0%. Deriving both from one line of output is the only way this stays
/// caught.
#[test]
fn the_trivial_percentage_matches_the_header_count() {
    let out = stdout(&run(&["scan", "-v", "fixtures/npm-m.package-lock.json"]));

    let header = out
        .lines()
        .find(|l| l.contains(" packages   ("))
        .expect("header");
    let total: f64 = header
        .split_whitespace()
        .nth(1)
        .unwrap()
        .replace(',', "")
        .parse()
        .unwrap();

    let line = out
        .lines()
        .find(|l| l.contains("TRIVIAL"))
        .expect("trivial");
    let mut f = line
        .split_whitespace()
        .skip_while(|w| *w != "TRIVIAL")
        .skip(1);
    let hits: f64 = f.next().unwrap().replace(',', "").parse().unwrap();
    let printed = f.next().unwrap().trim_start_matches('(');

    assert_eq!(printed, format!("{:.1}%", 100.0 * hits / total), "{line}");
}

/// The JSON object has to carry everything the header prints, because a machine
/// reading it cannot go back and look. `workspace` is the one that was missing:
/// packages, direct and transitive are all third-party counts, so nothing among
/// them says how many first-party entries the reader set aside, and a monorepo
/// was indistinguishable from a flat project of the same size.
#[test]
fn json_carries_the_workspace_count() {
    for (fixture, expected) in [
        ("fixtures/npm-m.package-lock.json", 6.0),
        ("fixtures/npm-xl.package-lock.json", 14.0),
        ("fixtures/npm-s.package-lock.json", 0.0),
    ] {
        let out = stdout(&run(&["scan", fixture, "--format", "json"]));
        let v = stranger::json::parse(&out).expect("parses");
        assert_eq!(
            v.get("workspace").and_then(stranger::json::Value::as_f64),
            Some(expected),
            "{fixture}"
        );
    }
}

/// `--fail-on` is the reason this is a CI gate rather than a report, so the
/// property that matters is monotonicity: if a scan passes at some threshold it
/// must pass at every stricter one. A gate that fails at `high` and passes at
/// `medium` is worse than no gate, because the person who tightened it would
/// have been told their tree got safer.
///
/// Checked across every fixture rather than the poisoned one, because the
/// interesting cases are the trees whose worst finding is in the middle:
/// `uv-m` and `cargo-l` top out at medium, `npm-xs` at low.
#[test]
fn fail_on_is_monotone_across_every_fixture() {
    const LEVELS: [&str; 4] = ["low", "medium", "high", "critical"];

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut checked = 0;

    for entry in std::fs::read_dir(&dir).expect("fixtures/") {
        let path = entry.expect("entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "README.md" {
            continue;
        }

        let failed: Vec<bool> = LEVELS
            .iter()
            .map(|level| {
                let out = run(&["scan", "-q", "--fail-on", level, path.to_str().unwrap()]);
                assert_ne!(out.status.code(), Some(2), "{name} at {level}: usage error");
                out.status.code() == Some(1)
            })
            .collect();

        // Once it stops failing it must not start again: `true`s form a prefix.
        let first_pass = failed.iter().position(|f| !f).unwrap_or(LEVELS.len());
        assert!(
            failed[first_pass..].iter().all(|f| !f),
            "{name}: fail-on is not monotone across {LEVELS:?} — got {failed:?}"
        );
        checked += 1;
    }

    assert!(checked >= 16, "expected every fixture, checked {checked}");
}
