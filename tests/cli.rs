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
/// The same two files as arguments to `fixture`, which joins `fixtures/` itself.
const POISONED_FILE: &str = "poisoned.package-lock.json";
const CLEAN_FILE: &str = "npm-xs.package-lock.json";

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

/// A project with no third-party dependencies at all writes a valid v9 pnpm
/// lockfile with no `packages:` section in it. Rejecting that made "you depend
/// on nobody" look like "your lockfile is broken".
#[test]
fn a_pnpm_lock_with_no_packages_is_an_empty_tree() {
    let dir = scratch("cli_pnpm_empty");
    let path = dir.join("pnpm-lock.yaml");
    write(
        &path,
        "lockfileVersion: '9.0'\n\nsettings:\n  autoInstallPeers: true\n\nimporters:\n\n  .: {}\n",
    );
    let o = run(&["scan", path.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(stdout(&o).contains("0 packages"), "{}", stdout(&o));
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
///
/// `hostile.package-lock.json` is in the list because the other two prove
/// nothing: a fixture with no escape in it cannot fail this. That file carries
/// a version string of `1.0.0\x1b[2K\x1b[1A...`, which erased the finding
/// above it and the `HALLUCINATION RISK` heading with it, on a run that still
/// exited 1.
#[test]
fn piped_output_carries_no_escape_codes() {
    const HOSTILE: &str = "fixtures/hostile.package-lock.json";
    for args in [
        &["scan", POISONED][..],
        &["scan", POISONED, "--format", "json"][..],
        &["scan", HOSTILE][..],
        &["scan", HOSTILE, "--format", "json"][..],
        &["scan", HOSTILE, "-v"][..],
        &["tree", "lodahs", HOSTILE][..],
    ] {
        let out = stdout(&run(args));
        assert!(!out.contains('\u{1b}'), "ESC in {args:?}");
        // The C1 range too: U+009B is a one-byte CSI wherever Latin-1 is
        // still being decoded.
        assert!(
            !out.chars()
                .any(|c| matches!(c, '\0'..='\x1f' | '\x7f'..='\u{9f}') && c != '\n'),
            "control character in {args:?}"
        );
    }
}

/// A hostile name must not knock the column out of alignment either — the
/// escape bytes were being counted as display columns.
#[test]
fn a_hostile_name_does_not_break_the_columns() {
    let out = stdout(&run(&["scan", "fixtures/hostile.package-lock.json", "-v"]));
    // Character offsets, not byte offsets: U+FFFD is three bytes and one
    // column, which is the entire point of replacing rather than dropping.
    let details: Vec<usize> = out
        .lines()
        .filter_map(|l| l.find("not in corpus").map(|b| l[..b].chars().count()))
        .collect();
    // Two, not four. `bell` and `csi` used to be findings here and are exactly
    // the false positives the length budget removed: at three and four
    // characters they get no edits at all, because a name that short is within
    // two edits of something in every registry. Alignment is what this test is
    // about and two rows still test it.
    assert!(details.len() >= 2, "{out}");
    assert!(
        details.windows(2).all(|w| w[0] == w[1]),
        "detail column is ragged: {details:?}\n{out}"
    );
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
    // Drop the milliseconds and nothing else. Replacing the whole `risk`
    // line — which is what this did — also dropped the risk score out of the
    // comparison, so the one number on that line worth pinning was the one
    // number not being pinned.
    let strip_timing = |s: String| {
        s.lines()
            .map(|l| match (l.find("    "), l.contains("risk ")) {
                (Some(_), true) => {
                    let mut kept: Vec<&str> = l.split("    ").collect();
                    kept.retain(|part| !part.trim_end().ends_with("ms"));
                    kept.join("    ")
                }
                _ => l.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let once = strip_timing(stdout(&run(&["scan", "fixtures"])));
    assert!(
        once.contains("risk 8") || once.contains("risk 7"),
        "the score survives the timing strip, or this test compares nothing"
    );
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

/// Both spellings of an option value. `--format=json` is what a person types
/// first and it used to be an "unknown option".
#[test]
fn options_take_an_equals_sign() {
    let spaced = run(&[
        "scan",
        "fixtures/npm-s.package-lock.json",
        "--format",
        "json",
    ]);
    let joined = run(&["scan", "fixtures/npm-s.package-lock.json", "--format=json"]);
    assert_eq!(stdout(&spaced), stdout(&joined));

    let gate = run(&[
        "scan",
        "fixtures/poisoned.package-lock.json",
        "--fail-on=critical",
    ]);
    assert_eq!(gate.status.code(), Some(1));
}

/// A switch given a value is a mistake, not a no-op — silently ignoring it
/// lets somebody believe they turned colour off.
#[test]
fn a_switch_refuses_a_value() {
    let o = run(&["scan", "fixtures", "--no-color=yes"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("takes no value"), "{}", stderr(&o));
}

/// Without `--`, a directory named `-v` is unreachable.
#[test]
fn a_double_dash_ends_the_options() {
    let dir = scratch("dashdash");
    let odd = dir.join("-v");
    write(
        &odd.join("package-lock.json"),
        &fixture("npm-s.package-lock.json"),
    );

    let o = Command::new(bin())
        .args(["scan", "--", "-v"])
        .current_dir(&dir)
        .output()
        .expect("stranger should run");
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).contains("package-lock.json"), "{}", stdout(&o));
}

/// `stranger scan --fail-on critical | head -1` has to keep the answer it
/// already computed. Every write after `head` exits is EPIPE, and mapping all
/// of them to success reported a tree full of criticals as clean.
#[test]
fn a_closed_pipe_does_not_erase_the_gate() {
    use std::process::Stdio;
    let mut child = Command::new(bin())
        .args(["scan", "fixtures", "--fail-on", "critical"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    // Drop the read end without reading it: every write the child makes now
    // fails with EPIPE, which is exactly what `| head -1` looks like.
    drop(child.stdout.take());
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(1), "the gate answer survives the pipe");
}

/// `/dev/null` exists. Reporting "no such file or directory" about it sends
/// somebody looking for a typo that is not there.
#[test]
fn a_device_node_is_not_reported_as_missing() {
    let o = run(&["scan", "/dev/null"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        stderr(&o).contains("not a regular file or directory"),
        "{}",
        stderr(&o)
    );
    let gone = run(&["scan", "/tmp/stranger-no-such-path-here"]);
    assert!(stderr(&gone).contains("no such file or directory"));
}

/// `std::env::args()` panics on a non-UTF-8 argument, and on Linux argv is
/// arbitrary bytes. It aborted the process, exit 134, before any of our code
/// ran.
#[cfg(unix)]
#[test]
fn a_non_utf8_argument_is_a_usage_error_not_an_abort() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let o = Command::new(bin())
        .arg("scan")
        .arg(OsStr::from_bytes(b"/tmp/\xff-nope"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("stranger should run");
    assert_eq!(o.status.code(), Some(2), "not a signal, not 134");
    assert!(stderr(&o).contains("not valid UTF-8"), "{}", stderr(&o));
}

/// The two output surfaces have to agree about the order of the same
/// findings. `report::human` sorts its blocks by `Rule::rank`; the JSON array
/// was in call order, which is not the same list on a tree where a later rule
/// outranks an earlier one.
#[test]
fn json_findings_are_in_report_order() {
    let o = run(&[
        "scan",
        "fixtures/npm-xl.package-lock.json",
        "--format",
        "json",
    ]);
    let body = stdout(&o);
    let rules: Vec<&str> = body
        .match_indices("\"rule\":\"")
        .map(|(i, _)| {
            let rest = &body[i + 8..];
            &rest[..rest.find('"').expect("closed string")]
        })
        .collect();
    let rank = |r: &str| match r {
        "slopsquat" => 0,
        "install-script" => 1,
        "trivial" => 2,
        "drift" => 3,
        "pinning" => 4,
        other => panic!("unknown rule {other}"),
    };
    assert!(
        rules.windows(2).all(|w| rank(w[0]) <= rank(w[1])),
        "{rules:?}"
    );
}

/// The machine-readable surface has to be byte-identical between two runs of
/// the same tree, because that is the whole justification for cutting
/// `stranger diff` — DECISIONS.md offers `diff <(stranger scan a --format
/// json) <(stranger scan b --format json)` in its place. `elapsed_ms` made
/// that diff print a difference every single time.
#[test]
fn json_is_byte_identical_between_runs() {
    let once = stdout(&run(&["scan", "fixtures", "--format", "json"]));
    assert!(!once.contains("elapsed"), "no clock in the machine surface");
    for _ in 0..3 {
        assert_eq!(
            stdout(&run(&["scan", "fixtures", "--format", "json"])),
            once
        );
    }
    // The human report keeps its timing, because "41ms" is half the pitch.
    assert!(stdout(&run(&["scan", POISONED])).contains("ms    third-party"));
}

/// Every reader computes `has_integrity` and, until this landed, nothing read
/// it — while README LIMITS claimed the tool reports whether the field is
/// present. Presence only: std has no crypto, so no sha512 is ever computed.
#[test]
fn json_carries_the_integrity_count() {
    let out = stdout(&run(&[
        "scan",
        "fixtures/npm-s.package-lock.json",
        "--format",
        "json",
    ]));
    let n: usize = out
        .split("\"integrity\":")
        .nth(1)
        .and_then(|rest| rest.split(&[',', '}'][..]).next())
        .expect("an integrity count")
        .parse()
        .expect("a number");
    assert!(n > 0, "npm records an integrity for every registry entry");
    // requirements.txt records no hashes at all, and saying 0 is the honest
    // answer rather than an omission.
    let flat = stdout(&run(&[
        "scan",
        "fixtures/reqs-s.requirements.txt",
        "--format",
        "json",
    ]));
    assert!(flat.contains("\"integrity\":0"), "{flat}");
}

/// chmod 000 does nothing to root, and an assertion that cannot fail is worse
/// than no test. std has no `geteuid` and this crate forbids the `unsafe` an
/// FFI one would need, so ask the filesystem: take the permissions away and
/// see whether they went.
#[cfg(unix)]
fn lock_out(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    std::fs::read_dir(dir).is_err()
}

#[cfg(unix)]
fn unlock(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
}

/// The worst bug this tool could have: a directory it cannot enter, reported as
/// clean. `read_dir` failing came back from the walk as an empty vec, which is
/// the value an empty directory produces, so a poisoned lockfile behind a 000
/// directory printed "no lockfile" and exited 0 under `--fail-on critical`.
#[cfg(unix)]
#[test]
fn an_unreadable_root_exits_two() {
    let dir = scratch("cli_locked_root");
    write(&dir.join("package-lock.json"), &fixture(POISONED_FILE));
    if !lock_out(&dir) {
        return; // root reads anything
    }

    let o = run(&["scan", dir.to_str().unwrap(), "--fail-on", "critical"]);
    let json = run(&["scan", dir.to_str().unwrap(), "--format", "json"]);
    // Restored before the assertions, or a failing run leaves behind a
    // directory the next `scratch` call cannot remove.
    unlock(&dir);
    let (out, json_out) = (stdout(&o), stdout(&json));

    assert_eq!(o.status.code(), Some(2), "{out}");
    assert!(out.contains("could not look inside"), "{out}");
    assert!(
        !out.contains("no lockfile"),
        "an unopenable directory is not an empty one: {out}"
    );
    // Still one JSON object per line, and the object names the path.
    assert_eq!(json.status.code(), Some(2));
    let v = stranger::json::parse(json_out.trim()).expect("blind-spot line parses");
    assert_eq!(
        v.get("unreadable")
            .and_then(stranger::json::Value::as_array)
            .map(<[_]>::len),
        Some(1),
        "{json_out}"
    );
}

/// An unreadable *subdirectory* is the judgement call, and it goes the same
/// way. The sibling scanned fine and its findings still print; the code is
/// still 2, because `--fail-on` is being asked about a list that is short by an
/// unknown number of lockfiles and neither 0 nor 1 answers that honestly.
#[cfg(unix)]
#[test]
fn an_unreadable_subdirectory_exits_two_beside_a_readable_sibling() {
    let dir = scratch("cli_locked_subdir");
    write(&dir.join("open/package-lock.json"), &fixture(CLEAN_FILE));
    write(&dir.join("shut/package-lock.json"), &fixture(POISONED_FILE));
    let shut = dir.join("shut");
    if !lock_out(&shut) {
        return;
    }

    let o = run(&["scan", dir.to_str().unwrap(), "--fail-on", "critical"]);
    unlock(&shut);
    let out = stdout(&o);

    assert!(out.contains("could not look inside"), "{out}");
    assert!(
        out.contains("packages   ("),
        "the sibling still gets reported: {out}"
    );
    assert_eq!(o.status.code(), Some(2), "{out}");
}

/// Eight lockfiles in a directory and stranger said "no lockfile in .". Naming
/// a format it will not read is not reading it; it is the difference between a
/// declared cut and a silent one, and the FAQ makes the declaration a condition
/// of reading these files at all.
#[test]
fn lockfiles_with_no_reader_are_named_not_ignored() {
    let dir = scratch("cli_unsupported");
    for name in [
        "yarn.lock",
        "go.sum",
        "Gemfile.lock",
        "composer.lock",
        "Pipfile.lock",
        "pdm.lock",
        "bun.lockb",
        "gradle.lockfile",
    ] {
        write(&dir.join(name), "x");
    }

    let o = run(&["scan", dir.to_str().unwrap(), "--fail-on", "critical"]);
    let out = stdout(&o);
    assert_eq!(o.status.code(), Some(0), "{out}");
    for named in ["yarn.lock", "go.sum", "gradle.lockfile", "bun.lockb"] {
        assert!(out.contains(named), "{named} missing from:\n{out}");
    }
    assert!(out.contains("found but not read"), "{out}");

    // JSON says the same thing on one parseable line rather than in prose.
    let json = stdout(&run(&["scan", "--format", "json", dir.to_str().unwrap()]));
    let v = stranger::json::parse(json.trim()).expect("parses");
    assert_eq!(
        v.get("unsupported")
            .and_then(stranger::json::Value::as_array)
            .map(<[_]>::len),
        Some(8),
        "{json}"
    );
}

/// `walk::MAX_DEPTH`, thirteen skip-list names and every hidden directory are
/// three silent blind spots. A lockfile in `.ci/` or under `dist/` was
/// invisible, with `--fail-on critical` returning 0 and nothing said anywhere.
#[test]
fn verbose_names_the_directories_it_would_not_enter() {
    let dir = scratch("cli_skipped");
    write(&dir.join("package-lock.json"), &fixture(CLEAN_FILE));
    write(&dir.join(".ci/package-lock.json"), &fixture(POISONED_FILE));
    write(
        &dir.join("dist/app/package-lock.json"),
        &fixture(POISONED_FILE),
    );

    let quiet = stdout(&run(&["scan", dir.to_str().unwrap()]));
    assert!(
        !quiet.contains("not descended into"),
        "policy stays behind -v: {quiet}"
    );

    let loud = stdout(&run(&["scan", "-v", dir.to_str().unwrap()]));
    assert!(loud.contains("not descended into (2)"), "{loud}");
    assert!(loud.contains(".ci") && loud.contains("hidden"), "{loud}");
    assert!(
        loud.contains("dist") && loud.contains("skip list"),
        "{loud}"
    );
}
/// A rule that could not fire is not the same claim as a rule that fired and
/// found nothing, and both printed as silence. `install_script` is hardcoded
/// false in the poetry, uv, Cargo and pnpm readers because those four files
/// record no such flag, so `stranger scan poetry.lock` reported no install
/// scripts on a question it had never asked — exit 0 at every level.
#[test]
fn a_rule_with_no_signal_in_this_format_says_so() {
    let out = stdout(&run(&["scan", "fixtures/poetry-m.poetry.lock"]));
    assert!(out.contains("no findings"), "{out}");
    assert!(
        out.contains("INSTALL SCRIPTS        — no signal in this format"),
        "{out}"
    );
    // Go has no corpus, so the name rules cannot speak about a go.mod either.
    let go = stdout(&run(&["scan", "fixtures/gomod-m.go.mod"]));
    assert!(go.contains("HALLUCINATION RISK"), "{go}");
    assert!(go.contains("no signal in this format"), "{go}");

    // npm records the flag, so it must not be on the list there — a report
    // that says "no signal" about everything says nothing.
    let npm = stdout(&run(&["scan", "fixtures/npm-xs.package-lock.json"]));
    assert!(!npm.contains("INSTALL SCRIPTS"), "{npm}");
    assert!(
        npm.contains("UNPINNED               — no signal in this format"),
        "npm resolves every entry, so pinning has nothing to read: {npm}"
    );

    // Not a finding, and it must never become one: nothing here moves the exit
    // code, because an unasked question is not evidence.
    for level in ["low", "medium", "high", "critical"] {
        assert_eq!(
            run(&["scan", "fixtures/gomod-m.go.mod", "--fail-on", level])
                .status
                .code(),
            Some(0),
            "{level}"
        );
    }
}

/// The JSON half. A consumer reading `"findings": []` as "clean" is right about
/// the rules that ran and wrong about the ones that could not, and it has no
/// other way to find out.
#[test]
fn json_lists_the_rules_that_could_not_fire() {
    let na = |fixture: &str| -> Vec<String> {
        let out = stdout(&run(&["scan", fixture, "--format", "json"]));
        stranger::json::parse(&out)
            .expect("parses")
            .get("not_applicable")
            .and_then(stranger::json::Value::as_array)
            .expect("not_applicable")
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    assert_eq!(
        na("fixtures/poetry-m.poetry.lock"),
        ["install-script", "pinning"]
    );
    assert_eq!(
        na("fixtures/gomod-m.go.mod"),
        ["slopsquat", "install-script", "pinning"]
    );
    assert_eq!(
        na("fixtures/pnpm-l.pnpm-lock.yaml"),
        ["install-script", "pinning"]
    );
    // requirements.txt is the one file here carrying a specifier rather than a
    // resolution, so it is the only one `pinning` can speak about — and the
    // only one whose list is a single entry.
    assert_eq!(na("fixtures/reqs-s.requirements.txt"), ["install-script"]);
    // package-lock.json is the only file that records install scripts, and it
    // still cannot answer `pinning`: every entry resolves to one version.
    assert_eq!(na("fixtures/npm-xs.package-lock.json"), ["pinning"]);
    assert_eq!(
        na("fixtures/cargo-s.Cargo.lock"),
        ["install-script", "pinning"]
    );
}
