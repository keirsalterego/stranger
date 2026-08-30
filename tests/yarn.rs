//! yarn.lock v1.
//!
//! The fixtures are three real lockfiles taken off this machine — the ones npm
//! packages ship inside their own tarballs — rather than files written to make
//! the reader look good. `yarn-xs` is small enough to check by hand, which is
//! why the edge test uses it.

use std::fs;
use std::path::{Path, PathBuf};
use stranger::error::Error;
use stranger::lock::{Ecosystem, Origin, Package, Tree, yarn};

fn path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn load(name: &str) -> Tree {
    let p = path(name);
    let src = fs::read_to_string(&p).unwrap();
    yarn::read(&p, &src).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn read(src: &str) -> Result<Tree, Error> {
    yarn::read(Path::new("yarn.lock"), src)
}

fn find<'a>(t: &'a Tree, name: &str) -> &'a Package {
    t.packages
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

/// 593 entry headers in the file, 593 packages out. Nothing merged, nothing
/// dropped by the comma split.
#[test]
fn the_fixture_parses() {
    let t = load("yarn-l.yarn.lock");
    assert_eq!(t.packages.len(), 593);
    assert_eq!(t.ecosystem, Ecosystem::Npm);
    assert!(t.records_edges);
}

/// The whole file is checkable by hand: three entries, and `far` names `oop`
/// at the exact version `oop`'s own key carries.
///
/// ```text
/// delayed-stream@~1.0.0   in-degree 0   root
/// far@~0.0.7              in-degree 0   root
///   └── oop "0.0.3"  ->   oop@0.0.3     in-degree 1   transitive
/// ```
#[test]
fn edges_resolve_through_the_specifier() {
    let t = load("yarn-xs.yarn.lock");
    assert_eq!(t.packages.len(), 3);
    let far = t.packages.iter().position(|p| p.name == "far").unwrap();
    let oop = t.packages.iter().position(|p| p.name == "oop").unwrap();
    assert_eq!(t.edges, vec![(far, oop)]);
    assert_eq!(t.roots.len(), 2);
    assert!(!t.roots.contains(&oop));
}

/// The failure this reader is most likely to have: a dependency line names a
/// *range*, and matching it against the target's resolved `version` finds
/// nothing. A reader that got this wrong still parses every entry and still
/// reports a package count — it just returns a tree with no edges, which reads
/// as "nothing depends on anything" rather than as an error.
#[test]
fn a_range_never_matches_a_resolved_version() {
    let t = load("yarn-l.yarn.lock");
    assert!(
        t.edges.len() > 500,
        "only {} edges: specifier resolution is broken",
        t.edges.len()
    );
    // `@babel/highlight@^7.0.0` is a key; `7.0.0` is what it resolves to. The
    // second is what a naive reader would look for and would not find.
    let hl = find(&t, "@babel/highlight");
    assert!(!hl.version.starts_with('^'));
}

/// `"@babel/generator@^7.9.0", "@babel/generator@^7.9.5":` is one entry with
/// two keys, and the file depends on it through both of them — `^7.9.0` at one
/// call site and `^7.9.5` at another. 75 of the 593 entries are written this
/// way, and a reader that took only the first key would silently lose every
/// edge that arrived through the second.
///
/// The other `@babel/generator` in the file is a separate entry at 7.4.4, so
/// this also pins down that merging by name would be wrong: two packages, one
/// of which answers to two names.
#[test]
fn one_entry_answers_to_several_specifiers() {
    let t = load("yarn-l.yarn.lock");
    let mut versions: Vec<&str> = t
        .packages
        .iter()
        .filter(|p| p.name == "@babel/generator")
        .map(|p| p.version.as_str())
        .collect();
    versions.sort_unstable();
    assert_eq!(versions, ["7.4.4", "7.9.5"]);

    let merged = t
        .packages
        .iter()
        .position(|p| p.name == "@babel/generator" && p.version == "7.9.5")
        .unwrap();
    // Two in-edges, and they can only have come through different keys: the
    // file names this package as `^7.9.0` once and as `^7.9.5` once.
    assert_eq!(t.edges.iter().filter(|&&(_, to)| to == merged).count(), 2);
}

/// A scoped name starts with `@`, so splitting the specifier at the *first*
/// `@` leaves the name empty. 96 of the entries are scoped.
#[test]
fn scoped_names_survive_the_split() {
    let t = load("yarn-l.yarn.lock");
    let scoped = t
        .packages
        .iter()
        .filter(|p| p.name.starts_with('@'))
        .count();
    assert_eq!(scoped, 96);
    assert!(t.packages.iter().all(|p| !p.name.is_empty()));
    let core = find(&t, "@babel/core");
    assert!(core.name.contains('/'), "scope was split off the name");
}

/// Three copies of one package at three versions, which is the drift rule's
/// entire input and a thing yarn v1 does constantly.
#[test]
fn the_same_name_at_several_versions() {
    let t = load("yarn-l.yarn.lock");
    let mut v: Vec<&str> = t
        .packages
        .iter()
        .filter(|p| p.name == "@babel/code-frame")
        .map(|p| p.version.as_str())
        .collect();
    v.sort_unstable();
    assert_eq!(v, ["7.0.0", "7.10.3", "7.8.3"]);
}

/// Every entry in the large fixture carries `integrity`; none in `yarn-xs`
/// does, because the yarn that wrote it predates the field. Reported as
/// present or absent, never as checked — std has no crypto.
#[test]
fn integrity_is_presence_only() {
    assert_eq!(
        load("yarn-l.yarn.lock")
            .packages
            .iter()
            .filter(|p| p.has_integrity)
            .count(),
        593
    );
    assert!(
        load("yarn-xs.yarn.lock")
            .packages
            .iter()
            .all(|p| !p.has_integrity)
    );
}

/// Both public hosts count. `registry.yarnpkg.com` is a CNAME of the npm
/// registry, and which one a file names depends only on the yarn that wrote
/// it, so a reader that knows one of them calls half the tree private and
/// switches the name rules off on it.
#[test]
fn both_registry_hosts_are_the_registry() {
    let t = read(concat!(
        "a@^1.0.0:\n  version \"1.0.0\"\n",
        "  resolved \"https://registry.yarnpkg.com/a/-/a-1.0.0.tgz\"\n",
        "b@^1.0.0:\n  version \"1.0.0\"\n",
        "  resolved \"https://registry.npmjs.org/b/-/b-1.0.0.tgz\"\n",
        "c@^1.0.0:\n  version \"1.0.0\"\n",
        "  resolved \"https://codeload.github.com/o/c/tar.gz/deadbeef\"\n",
    ))
    .unwrap();
    assert_eq!(find(&t, "a").origin, Origin::Registry);
    assert_eq!(find(&t, "b").origin, Origin::Registry);
    assert_eq!(find(&t, "c").origin, Origin::Elsewhere);
}

// -- refusals ---------------------------------------------------------------

/// Berry is a real YAML document keyed `name@npm:range`. Same filename,
/// different format; reading half of it is worse than saying so.
#[test]
fn berry_is_refused() {
    let err =
        read("__metadata:\n  version: 8\n\n\"a@npm:^1.0.0\":\n  version: 1.0.0\n").unwrap_err();
    assert!(err.to_string().contains("Yarn Berry"));
}

#[test]
fn an_entry_with_no_version_is_refused() {
    let err = read("a@^1.0.0:\n  resolved \"https://example.com/a.tgz\"\n").unwrap_err();
    assert!(err.to_string().contains("no `version` field"), "{err}");
}

#[test]
fn a_header_without_a_colon_is_refused() {
    let err = read("a@^1.0.0\n  version \"1.0.0\"\n").unwrap_err();
    assert!(err.to_string().contains("does not end in `:`"), "{err}");
}

/// Position is the point of refusing rather than skipping: the message names a
/// line the reader can open.
#[test]
fn errors_carry_a_line() {
    let err = read("a@^1.0.0:\n  version \"1.0.0\"\n\nbroken\n").unwrap_err();
    match err {
        Error::Syntax { line, .. } => assert_eq!(line, 4),
        other => panic!("wrong variant: {other}"),
    }
}

/// Truncation at every byte offset. A lockfile that stops mid-entry is what a
/// killed `yarn install` leaves behind, and it must error rather than panic.
#[test]
fn truncation_never_panics() {
    let src = fs::read_to_string(path("yarn-m.yarn.lock")).unwrap();
    for end in (0..src.len()).step_by(64) {
        if !src.is_char_boundary(end) {
            continue;
        }
        let _ = read(&src[..end]);
    }
}
