//! Colour decisions, column widths, and the promise that a pipe gets plain text.

use std::path::PathBuf;
use std::time::Duration;
use stranger::lock::{Ecosystem, Origin, Package, Pin, Tree};
use stranger::report;
use stranger::rules::{Finding, Rule, Severity};
use stranger::term::{self, Style, Term};

// -- the precedence table ---------------------------------------------------

#[test]
fn tty_decides_alone() {
    assert!(term::decide(false, None, None, true));
    assert!(!term::decide(false, None, None, false));
}

#[test]
fn no_color_beats_tty() {
    assert!(!term::decide(false, Some("1"), None, true));
}

#[test]
fn force_beats_pipe() {
    assert!(term::decide(false, None, Some("1"), false));
}

#[test]
fn no_color_beats_force() {
    assert!(!term::decide(false, Some("1"), Some("1"), true));
}

#[test]
fn flag_beats_all() {
    assert!(!term::decide(true, None, Some("1"), true));
}

/// `NO_COLOR=` is how a shell clears a variable it cannot `unset`. Present but
/// empty has to mean nothing was said, or every such shell loses colour.
#[test]
fn empty_says_nothing() {
    assert!(term::decide(false, Some(""), None, true));
    assert!(!term::decide(false, Some(""), None, false));
    assert!(!term::decide(false, None, Some(""), false));
}

// -- widths -----------------------------------------------------------------

#[test]
fn width_counts_chars() {
    assert_eq!("café".len(), 5);
    assert_eq!(term::width("café"), 4);
    assert_eq!(term::width("ünïcödé-pkg"), 11);
}

#[test]
fn column_takes_the_widest() {
    assert_eq!(term::column(["a", "bbbb", "cc"], 0), 4);
    assert_eq!(term::column(["a", "bbbb", "cc"], 10), 10);
    assert_eq!(term::column(std::iter::empty(), 3), 3);
}

#[test]
fn pad_reaches_the_column() {
    assert_eq!(term::pad("ab", 5), "ab   ");
    assert_eq!(term::pad("café", 6), "café  ");
    assert_eq!(term::pad("abcdef", 3), "abcdef");
}

// -- painting ---------------------------------------------------------------

#[test]
fn paint_off_is_the_input() {
    assert_eq!(Term::new(false).paint(Style::Red, "x"), "x");
}

#[test]
fn paint_on_resets() {
    assert_eq!(Term::new(true).paint(Style::Red, "x"), "\x1b[31mx\x1b[0m");
}

// -- the rendered report ----------------------------------------------------

fn tree() -> Tree {
    let pkg = |name: &str| Package {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        key: format!("node_modules/{name}"),
        dev: false,
        optional: false,
        first_party: false,
        install_script: false,
        has_integrity: true,
        pinned: Pin::Exact,
        origin: Origin::Registry,
    };
    Tree {
        ecosystem: Ecosystem::Npm,
        source: PathBuf::from("fixtures/package-lock.json"),
        packages: vec![pkg("chalck"), pkg("ünïcödé-package-name")],
        edges: Vec::new(),
        roots: vec![0, 1],
        records_edges: true,
    }
}

fn finding(package: &str, version: &str) -> Finding {
    Finding {
        rule: Rule::Slopsquat,
        severity: Severity::Critical,
        package: package.to_string(),
        version: version.to_string(),
        detail: "not in corpus".to_string(),
    }
}

fn render(color: bool, findings: &[Finding]) -> String {
    let mut buf = Vec::new();
    report::human(
        &mut buf,
        Term::new(color),
        &tree(),
        findings,
        Duration::from_millis(7),
        false,
        false,
    )
    .unwrap();
    String::from_utf8(buf).unwrap()
}

fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn piped_output_has_no_escapes() {
    let out = render(false, &[finding("chalck", "5.3.0")]);
    assert!(!out.contains('\x1b'), "{out:?}");
}

#[test]
fn colour_marks_severity() {
    let out = render(true, &[finding("chalck", "5.3.0")]);
    assert!(out.contains("\x1b[31m⚠\x1b[0m"), "{out:?}");
    assert!(out.contains("\x1b[31mHALLUCINATION RISK"), "{out:?}");
}

/// Colour adds bytes and nothing else. Strip the escapes back out and the two
/// renders have to be the same string, character for character.
#[test]
fn colour_off_is_the_plain_render() {
    let findings = [finding("chalck", "5.3.0")];
    assert_eq!(strip(&render(true, &findings)), render(false, &findings));
}

/// A name past the 24-column floor moves the detail column for every row, and
/// it is 20 characters in 26 bytes — padding by `len()` would leave this one
/// six columns short of the others.
#[test]
fn multibyte_name_widens_the_column() {
    let findings = [
        finding("chalck", "5.3.0"),
        finding("ünïcödé-package-name", "1.0.0"),
    ];
    let out = render(false, &findings);
    let detail_cols: Vec<usize> = out
        .lines()
        .filter_map(|l| l.find("not in corpus").map(|b| l[..b].chars().count()))
        .collect();
    // 5 indent + the widest label ("ünïcödé-package-name@1.0.0", 26) + 1 gap.
    assert_eq!(detail_cols, vec![32, 32]);
}

#[test]
fn short_names_keep_the_floor() {
    let out = render(false, &[finding("chalck", "5.3.0")]);
    let col = out
        .lines()
        .find_map(|l| l.find("not in corpus").map(|b| l[..b].chars().count()));
    assert_eq!(col, Some(30));
}

/// Blocks come out in `Rule::rank` order — not the order the findings arrived
/// in, which is why they are fed in backwards here, and not severity order,
/// which is why `UNPINNED` still prints last while holding the only `High` and
/// `TRIVIAL` still prints above it on a `Low`.
#[test]
fn blocks_print_in_rank_order_whatever_order_they_arrive_in() {
    let at = |rule, severity| Finding {
        rule,
        severity,
        package: "p".to_string(),
        version: "1.0.0".to_string(),
        detail: "d".to_string(),
    };
    let out = render(
        false,
        &[
            at(Rule::Pinning, Severity::High),
            at(Rule::Drift, Severity::Medium),
            at(Rule::Trivial, Severity::Low),
            at(Rule::InstallScript, Severity::High),
            at(Rule::Slopsquat, Severity::Critical),
        ],
    );
    let headings: Vec<&str> = out
        .lines()
        .filter_map(|l| l.strip_prefix("  ⚠  "))
        // The heading is the cell before the column gap; headings themselves
        // never carry a double space.
        .filter_map(|l| l.split("  ").next())
        .collect();
    assert_eq!(
        headings,
        [
            "HALLUCINATION RISK",
            "INSTALL SCRIPTS",
            "TRIVIAL",
            "VERSION DRIFT",
            "UNPINNED",
        ],
        "{out}"
    );
}

// -- digit grouping ---------------------------------------------------------

#[test]
fn digit_grouping() {
    assert_eq!(report::thousands(0), "0");
    assert_eq!(report::thousands(1), "1");
    assert_eq!(report::thousands(999), "999");
    assert_eq!(report::thousands(1000), "1,000");
    assert_eq!(report::thousands(1234567), "1,234,567");
}
