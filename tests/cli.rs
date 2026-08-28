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
