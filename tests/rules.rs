//! Counts here were measured with `jq` against the fixtures before the rules
//! existed, not read back off the tool. If a change moves one, the change has
//! to argue with the lockfile.

use std::fs;
use std::path::{Path, PathBuf};
use stranger::lock::{Tree, npm};
use stranger::rules::{Finding, Severity, drift, scripts, trivial};

fn load(name: &str) -> Tree {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let src = fs::read_to_string(&path).unwrap();
    npm::read(&path, &src).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn inline(src: &str) -> Tree {
    npm::read(Path::new("inline.package-lock.json"), src).unwrap()
}

fn names(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.package.as_str()).collect()
}

fn detail_of<'a>(findings: &'a [Finding], package: &str) -> &'a str {
    findings
        .iter()
        .find(|f| f.package == package)
        .unwrap_or_else(|| panic!("no finding for {package}"))
        .detail
        .as_str()
}

/// A workspace member with a build script, and the symlink npm leaves in
/// `node_modules` pointing at it. Both are code somebody in this repo wrote,
/// and the root's own `hasInstallScript` is your build rather than a stranger's.
const WORKSPACE: &str = r#"{
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "root", "hasInstallScript": true, "dependencies": { "is-odd": "^1" } },
    "pkg/a": { "name": "a", "version": "1.0.0", "hasInstallScript": true },
    "node_modules/a": { "resolved": "pkg/a", "link": true },
    "node_modules/is-odd": { "version": "1.0.0" },
    "node_modules/x": { "version": "1.0.0", "dependencies": { "a": "^2" } },
    "node_modules/x/node_modules/a": { "version": "2.0.0" }
  }
}"#;

#[test]
fn install_script_counts() {
    assert_eq!(scripts::scan(&load("npm-xs.package-lock.json")).len(), 0);
    assert_eq!(scripts::scan(&load("npm-s.package-lock.json")).len(), 3);
    assert_eq!(scripts::scan(&load("npm-m.package-lock.json")).len(), 4);
    assert_eq!(scripts::scan(&load("npm-l.package-lock.json")).len(), 2);
    assert_eq!(scripts::scan(&load("npm-xl.package-lock.json")).len(), 8);
}

/// The flag is all lockfileVersion 3 carries. A detail that named a hook, a
/// command or an intent would be stranger inventing evidence.
#[test]
fn install_script_detail_describes_nothing() {
    let findings = scripts::scan(&load("npm-xl.package-lock.json"));
    for f in &findings {
        assert_eq!(f.severity, Severity::High);
        assert!(f.detail.contains("not the script"), "{}", f.detail);
    }
    // esbuild unpacks a binary; the line it gets is the line everything gets.
    assert_eq!(
        detail_of(&findings, "esbuild"),
        detail_of(&findings, "node-pty")
    );
}

/// A nested copy is installed and its hook runs, so both copies are reported.
#[test]
fn install_scripts_count_copies() {
    let findings = scripts::scan(&load("npm-xl.package-lock.json"));
    assert_eq!(
        names(&findings)
            .iter()
            .filter(|n| **n == "fsevents")
            .count(),
        2
    );
}

#[test]
fn drift_counts() {
    assert_eq!(drift::scan(&load("npm-xs.package-lock.json")).len(), 0);
    assert_eq!(drift::scan(&load("npm-s.package-lock.json")).len(), 30);
    assert_eq!(drift::scan(&load("npm-m.package-lock.json")).len(), 20);
    assert_eq!(drift::scan(&load("npm-l.package-lock.json")).len(), 55);
    assert_eq!(drift::scan(&load("npm-xl.package-lock.json")).len(), 76);
}

#[test]
fn drift_lists_the_versions() {
    let findings = drift::scan(&load("npm-s.package-lock.json"));
    assert_eq!(
        detail_of(&findings, "ansi-regex"),
        "2 versions: 5.0.1, 6.1.0"
    );
    assert_eq!(
        detail_of(&findings, "agent-base"),
        "3 versions: 5.1.1, 6.0.2, 7.1.4"
    );
}

#[test]
fn drift_reports_a_name_once() {
    let findings = drift::scan(&load("npm-xl.package-lock.json"));
    let mut seen = names(&findings);
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), before);
    // and never carries a single version it could not have chosen
    assert!(findings.iter().all(|f| f.version.is_empty()));
}

#[test]
fn trivial_counts() {
    assert_eq!(trivial::scan(&load("npm-xs.package-lock.json")).len(), 4);
    assert_eq!(trivial::scan(&load("npm-s.package-lock.json")).len(), 10);
    assert_eq!(trivial::scan(&load("npm-m.package-lock.json")).len(), 17);
    assert_eq!(trivial::scan(&load("npm-l.package-lock.json")).len(), 35);
    assert_eq!(trivial::scan(&load("npm-xl.package-lock.json")).len(), 29);
}

/// Clause one names them; clause two only guesses at a shape, and the detail
/// has to keep the two apart.
#[test]
fn trivial_separates_list_from_shape() {
    let findings = trivial::scan(&load("npm-xl.package-lock.json"));
    assert!(detail_of(&findings, "isarray").contains("one expression"));
    assert!(detail_of(&findings, "is-callable").contains("size not measured"));
}

/// A predicate name that needs help is not a one-liner, and the second half of
/// clause two is what says so. Both of these are in npm-xl.
#[test]
fn trivial_skips_predicates_with_dependencies() {
    let findings = trivial::scan(&load("npm-xl.package-lock.json"));
    let found = names(&findings);
    assert!(!found.contains(&"has-tostringtag"));
    assert!(!found.contains(&"is-glob"));
}

#[test]
fn known_list_is_sorted() {
    let mut sorted = trivial::KNOWN.to_vec();
    sorted.sort_unstable();
    assert_eq!(sorted, trivial::KNOWN);
    sorted.dedup();
    assert_eq!(sorted.len(), trivial::KNOWN.len());
}

#[test]
fn first_party_is_never_a_finding() {
    let tree = inline(WORKSPACE);
    assert!(scripts::scan(&tree).is_empty());
    assert_eq!(names(&trivial::scan(&tree)), ["is-odd"]);
    // The link entry carries no version, so `a` would look like it drifted
    // from 2.0.0 if first-party entries counted.
    assert!(drift::scan(&tree).is_empty());
}

/// Poisoning npm-l added three names and one of them installs with a script,
/// so the same package is reported twice by two rules that mean different
/// things. Neither drift nor trivial moved: a planted root name is a single
/// version and is not shaped like a micro-package.
#[test]
fn poisoned() {
    let clean = load("npm-l.package-lock.json");
    let dirty = load("poisoned.package-lock.json");

    let scripts = scripts::scan(&dirty);
    assert_eq!(scripts.len(), scripts::scan(&clean).len() + 1);
    assert!(names(&scripts).contains(&"lodahs"));

    assert_eq!(drift::scan(&dirty).len(), drift::scan(&clean).len());
    assert_eq!(trivial::scan(&dirty).len(), trivial::scan(&clean).len());
}
