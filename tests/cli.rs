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
