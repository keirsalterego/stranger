//! `stranger tree` — the co-occurrence rule's third clause, as output someone
//! can read.
//!
//! Driven through the built binary rather than the library, like `tests/cli.rs`
//! and for the same reason: the exit code and the bytes on stdout are the
//! contract, and neither is visible from inside `main`.

use std::path::PathBuf;
use std::process::{Command, Output};

fn bin() -> PathBuf {
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
const NPM_L: &str = "fixtures/npm-l.package-lock.json";
const NPM_XL: &str = "fixtures/npm-xl.package-lock.json";
const FLAT: &str = "fixtures/poisoned.requirements.txt";

/// The point of the whole subcommand. All three planted names are root-only,
/// and the output has to say so in the words the rule and the README use, or
/// the demo is one claim checked against a different claim.
#[test]
fn planted_names_have_no_parent() {
    for planted in ["chalck", "expres", "lodahs"] {
        let o = run(&["tree", planted, POISONED]);
        let out = stdout(&o);
        assert_eq!(o.status.code(), Some(0), "{out}");
        assert!(out.contains("in-degree 0"), "{planted}: {out}");
        assert!(out.contains("root-only, no parent"), "{planted}: {out}");
        assert!(out.contains("clause 3"), "{planted}: {out}");
        assert!(out.contains("depends on       nothing"), "{planted}: {out}");
    }
}

/// The control. A real package that real packages need has in-edges, and the
/// number is the one the slopsquat rule reads.
#[test]
fn a_real_package_has_parents() {
    let out = stdout(&run(&["tree", "express", NPM_L]));
    assert!(out.contains("in-degree 2"), "{out}");
    assert!(out.contains("express-rate-limit@8.6.0"), "{out}");
    assert!(out.contains("@modelcontextprotocol/sdk@1.29.0"), "{out}");
    // And the out-edges, drawn.
    assert!(out.contains("├─ accepts@2.0.0"), "{out}");
    assert!(out.contains("│  └─ mime-db@1.54.0"), "{out}");
}

/// npm records a duplicated package as a second entry under a nested key, and
/// version drift is one of the four things this tool reports. Picking one
/// version here would hide a finding a scan of the same file raises.
#[test]
fn every_version_is_shown() {
    let out = stdout(&run(&["tree", "semver", NPM_XL, "-q"]));
    let heads: Vec<&str> = out.lines().filter(|l| l.starts_with("  semver@")).collect();
    assert_eq!(heads.len(), 9, "nine entries, four versions: {out}");
    for v in [
        "semver@5.7.2",
        "semver@6.3.1",
        "semver@7.7.4",
        "semver@7.8.5",
    ] {
        assert!(out.contains(v), "{v} missing: {out}");
    }
    // The nested install path is what makes a second copy a second copy, so it
    // is printed next to the version.
    assert!(
        out.contains("node_modules/@babel/core/node_modules/semver"),
        "{out}"
    );
}

/// A name that is not there is an answer, not a crash. Exit 0, and say what is
/// close, because "no such package" on its own teaches people not to run it.
#[test]
fn a_missing_name_lists_what_is_close() {
    let o = run(&["tree", "lodashh", "fixtures"]);
    let out = stdout(&o);
    assert_eq!(o.status.code(), Some(0), "{out}");
    assert!(out.contains("no package named `lodashh`"), "{out}");
    assert!(out.contains("lodash"), "{out}");
    assert!(out.contains("d=1"), "{out}");
    // Distance is the rule's, not a second opinion: the planted `lodahs` is
    // two edits away and comes after the real name.
    assert!(out.find("d=1") < out.find("d=2"), "{out}");
}

#[test]
fn nothing_close_says_that_too() {
    let out = stdout(&run(&["tree", "qqqqqqqqqqqqqqqq", "fixtures"]));
    assert!(out.contains("no package named"), "{out}");
    assert!(out.contains("nothing within 2 edits"), "{out}");
}

/// README LIMITS: a flat format records no edges, so in-degree 0 there is the
/// file declining to say rather than a measurement. Printing a bare 0 would be
/// the exact confusion clause 3 exists to avoid.
#[test]
fn a_flat_file_says_it_has_no_graph() {
    let out = stdout(&run(&["tree", "requests-http", FLAT]));
    assert!(out.contains("no graph in this file"), "{out}");
    assert!(
        out.contains("requirements.txt records no dependency edges"),
        "{out}"
    );
    assert!(out.contains("clause 3 is vacuous"), "{out}");
    // No count is offered, only the sentence explaining why there is none.
    assert!(!out.contains("depended on by"), "nothing to measure: {out}");
    assert!(!out.contains("depends on"), "nothing to walk: {out}");
}

/// Real lockfiles have cycles in them — npm records peer dependencies both ways
/// round. The walk has to stop and it has to say that it stopped.
#[test]
fn a_cycle_is_named_not_followed() {
    let out = stdout(&run(&["tree", "eslint", NPM_XL, "--depth", "0"]));
    assert!(out.contains("· cycle"), "{out}");
    assert!(
        out.contains("(*)"),
        "a DAG repeats, and repeats are marked: {out}"
    );
}

/// The other stop. A cut tree that does not say where it was cut reads as a
/// package that depends on less than it does.
#[test]
fn the_depth_cut_announces_itself() {
    let out = stdout(&run(&["tree", "express", NPM_L, "--depth", "1"]));
    assert!(out.contains("28 direct, to depth 1"), "{out}");
    assert!(out.contains("more below, past --depth 1"), "{out}");
    let deep = stdout(&run(&["tree", "express", NPM_L, "--depth", "0"]));
    assert!(deep.contains("all the way down"), "{deep}");
    assert!(deep.lines().count() > out.lines().count(), "{deep}");
}

#[test]
fn json_shape() {
    let out = stdout(&run(&["tree", "express", NPM_L, "--format", "json"]));
    let v = stranger::json::parse(&out).expect("our own parser reads it");
    assert_eq!(v.get("query").and_then(|q| q.as_str()), Some("express"));
    assert_eq!(v.get("found").and_then(|f| f.as_bool()), Some(true));
    assert_eq!(v.get("depth").and_then(|d| d.as_f64()), Some(3.0));

    let occ = v
        .get("occurrences")
        .and_then(|o| o.as_array())
        .expect("occurrences");
    assert_eq!(occ.len(), 1);
    let one = &occ[0];
    assert_eq!(one.get("in_degree").and_then(|d| d.as_f64()), Some(2.0));
    assert_eq!(
        one.get("records_edges").and_then(|e| e.as_bool()),
        Some(true)
    );
    assert_eq!(
        one.get("parents")
            .and_then(|p| p.as_array())
            .map(<[_]>::len),
        Some(2)
    );
    let deps = one
        .get("dependencies")
        .and_then(|d| d.as_array())
        .expect("dependencies");
    assert_eq!(deps.len(), 28);
    // Nested, not flattened: a dependency carries its own dependencies.
    assert!(deps.iter().any(|d| {
        d.get("dependencies")
            .and_then(|n| n.as_array())
            .is_some_and(|n| !n.is_empty())
    }));
}

/// `in_degree` is null and not 0 on a flat format, because nobody measured 0.
#[test]
fn json_flat_file_has_a_null_in_degree() {
    let out = stdout(&run(&["tree", "requests-http", FLAT, "--format", "json"]));
    let v = stranger::json::parse(&out).expect("parses");
    let one = &v
        .get("occurrences")
        .and_then(|o| o.as_array())
        .expect("occ")[0];
    assert_eq!(
        one.get("records_edges").and_then(|e| e.as_bool()),
        Some(false)
    );
    assert_eq!(one.get("in_degree"), Some(&stranger::json::Value::Null));
}

#[test]
fn json_says_when_it_found_nothing() {
    let out = stdout(&run(&["tree", "lodashh", "fixtures", "--format", "json"]));
    let v = stranger::json::parse(&out).expect("parses");
    assert_eq!(v.get("found").and_then(|f| f.as_bool()), Some(false));
    let near = v.get("near").and_then(|n| n.as_array()).expect("near");
    assert!(!near.is_empty(), "{out}");
    assert_eq!(near[0].get("name").and_then(|n| n.as_str()), Some("lodash"));
}

/// The README promises a diff between two runs is a diff. `tree` prints no
/// timing at all, so this is byte-for-byte and not stripped of anything.
#[test]
fn two_runs_are_identical_bytes() {
    for args in [
        &["tree", "semver", "fixtures"][..],
        &["tree", "semver", "fixtures", "--format", "json"][..],
        &["tree", "express", "fixtures", "--depth", "0"][..],
        &["tree", "nothing-is-called-this", "fixtures"][..],
    ] {
        let once = run(args).stdout;
        for _ in 0..3 {
            assert_eq!(run(args).stdout, once, "{args:?}");
        }
    }
}

#[test]
fn piped_output_carries_no_escape_codes() {
    for args in [
        &["tree", "lodahs", POISONED][..],
        &["tree", "lodahs", POISONED, "--no-color"][..],
        &["tree", "express", NPM_L, "--format", "json"][..],
    ] {
        assert!(!stdout(&run(args)).contains('\u{1b}'), "{args:?}");
    }
}

#[test]
fn quiet_drops_the_prose_and_keeps_the_number() {
    let out = stdout(&run(&["tree", "lodahs", POISONED, "-q"]));
    assert!(out.contains("in-degree 0"), "{out}");
    assert!(!out.contains("clause 3"), "the explanation goes: {out}");
    assert!(!out.contains("757 packages"), "the header goes: {out}");
}

/// Usage mistakes are 2, never 1 — the same contract `scan` has.
#[test]
fn usage_mistakes_exit_two() {
    for args in [
        &["tree"][..],
        &["tree", "express", "no/such/path"][..],
        &["tree", "express", ".", "extra"][..],
        &["tree", "express", "--depth", "deep"][..],
        &["tree", "express", "--nonsense"][..],
        // Real flags, on the other command. Saying so beats "unknown option",
        // which sends somebody looking for a typo they did not make.
        &["tree", "express", "--fail-on", "high"][..],
        &["tree", "express", "-v"][..],
    ] {
        assert_eq!(run(args).status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn help_documents_tree() {
    let out = stdout(&run(&["--help"]));
    assert!(out.contains("stranger tree <pkg> [path]"), "{out}");
    assert!(out.contains("--depth"), "{out}");
}
