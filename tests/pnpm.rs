use std::fs;
use std::path::{Path, PathBuf};
use stranger::error::Error;
use stranger::lock::{Ecosystem, Origin, Package, Pin, Tree, pnpm};
use stranger::rules::slopsquat;

const FIXTURE: &str = "pnpm-l.pnpm-lock.yaml";

fn path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn load() -> Tree {
    let p = path(FIXTURE);
    let src = fs::read_to_string(&p).unwrap();
    pnpm::read(&p, &src).unwrap_or_else(|e| panic!("{FIXTURE}: {e}"))
}

fn read(src: &str) -> Result<Tree, Error> {
    pnpm::read(Path::new("pnpm-lock.yaml"), src)
}

fn find<'a>(t: &'a Tree, name: &str) -> &'a Package {
    t.packages
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn degree_of(t: &Tree, name: &str) -> u32 {
    let deg = t.in_degree();
    let i = t.packages.iter().position(|p| p.name == name).unwrap();
    deg[i]
}

/// 850 `resolution:` lines in the file, 850 packages out. `snapshots` has 850
/// entries too — one per package, because this project has a single importer
/// and never installs the same version twice with different peers.
#[test]
fn counts() {
    let t = load();
    assert_eq!(t.packages.len(), 850);
    assert_eq!(t.ecosystem, Ecosystem::Npm);
    assert_eq!(t.direct() + t.transitive(), 850);
}

/// Every `dependencies` and `optionalDependencies` line under `snapshots`
/// resolves. If the peer-suffix truncation were wrong the count would drop,
/// not error — which is why this is a number and not an `is_empty` check.
#[test]
fn edges() {
    let t = load();
    assert_eq!(t.edges.len(), 1851);
    // No self-loops, and every endpoint is in range.
    for &(from, to) in &t.edges {
        assert!(from < 850 && to < 850);
        assert_ne!(from, to);
    }
}

/// The clause the slopsquat rule leans on. A package nothing depends on is
/// suspicious; a package 36 things depend on is not, and the reader has to be
/// able to tell them apart.
#[test]
fn transitive_packages_have_parents() {
    let t = load();
    assert!(degree_of(&t, "call-bound") > 0);
    assert!(degree_of(&t, "es-errors") > 0);
    // `astro` is a direct dependency of the only importer and nothing in the
    // tree depends on it.
    assert_eq!(degree_of(&t, "astro"), 0);
    // The strong version: 24 of the 850 have no parent, and all 24 are among
    // the 29 the importer declares. Nothing in this tree is unreachable, which
    // is what a correct edge resolution looks like — get the peer suffixes
    // wrong and hundreds of packages float free, every one of them a candidate
    // for the slopsquat rule's third clause.
    let deg = t.in_degree();
    let orphans: Vec<usize> = (0..t.packages.len()).filter(|&i| deg[i] == 0).collect();
    assert_eq!(orphans.len(), 24);
    for i in orphans {
        assert!(
            t.roots.contains(&i),
            "{} reaches nothing",
            t.packages[i].key
        );
    }
}

/// The importer's own manifest is not evidence that a package exists, so its
/// dependencies land in `roots` rather than in `edges`.
#[test]
fn importer_dependencies_are_roots() {
    let t = load();
    assert_eq!(t.direct(), 29);
    for &r in &t.roots {
        assert!(!t.packages[r].first_party);
    }
    // Both halves of the importer are read: `marked` is a dependency,
    // `typescript` a devDependency.
    let names: Vec<&str> = t
        .roots
        .iter()
        .map(|&i| t.packages[i].name.as_str())
        .collect();
    assert!(names.contains(&"marked"));
    assert!(names.contains(&"typescript"));
}

/// A scoped name splits at the *last* `@`, and a peer suffix never splits at
/// all.
#[test]
fn scoped_names_survive_the_split() {
    let t = load();
    let core = find(&t, "@babel/core");
    assert_eq!(core.version, "7.27.1");
    assert_eq!(core.key, "@babel/core@7.27.1");
    assert_eq!(find(&t, "zwitch").version, "2.0.4");
    // Every key round-trips: name, '@', version.
    for p in &t.packages {
        assert_eq!(p.key, format!("{}@{}", p.name, p.version), "{}", p.key);
        assert!(!p.name.is_empty() && !p.version.is_empty());
    }
    // A quarter of the tree is scoped, which is the case a naive split loses.
    assert!(
        t.packages
            .iter()
            .filter(|p| p.name.starts_with('@'))
            .count()
            > 100
    );
}

/// `resolution: {integrity: sha512-…}` on all 850. Whether the hash is
/// *correct* is not something this tool claims to know.
#[test]
fn integrity_is_recorded_for_every_package() {
    let t = load();
    assert_eq!(t.packages.iter().filter(|p| p.has_integrity).count(), 850);
}

/// pnpm 9 dropped `requiresBuild` and never had anything else that means "runs
/// code at install time". `hasBin` is on 42 packages and is a different claim,
/// so the flag stays false and the install-script rule stays quiet.
#[test]
fn no_install_scripts_are_claimed() {
    let t = load();
    assert_eq!(t.packages.iter().filter(|p| p.install_script).count(), 0);
}

/// Move every flow collection onto the line below its key. `resolution: {…}`
/// becomes `resolution:` and then the brace, indented two further columns.
/// YAML says the two spell the same node.
fn reformatted(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + src.len() / 16);
    for line in src.lines() {
        let indent = line.len() - line.trim_start().len();
        match line.split_once(": ") {
            Some((key, value)) if value.starts_with(['{', '[']) => {
                out.push_str(key);
                out.push_str(":\n");
                out.push_str(&" ".repeat(indent + 2));
                out.push_str(value);
            }
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

fn slopsquat_findings(t: &Tree) -> Vec<String> {
    // A fixed three-name corpus rather than the compiled-in one, so this test
    // measures the reader and not the day the npm snapshot was taken. Sorted,
    // because `corpus::contains_in` binary-searches it.
    let names = ["chalk", "lodash", "react"];
    slopsquat::scan(
        t,
        slopsquat::Config {
            require_no_parent: true,
            corpus: Some(&names),
        },
    )
    .iter()
    .map(|f| format!("{} {}@{} · {}", f.severity, f.package, f.version, f.detail))
    .collect()
}

/// The same lockfile, spelled two legal ways, has to produce the same
/// findings. This is the property; everything else in this file is a proxy
/// for it.
///
/// It did not hold, and it did not hold *quietly*. Read as a block mapping,
/// the own-line brace produced the key `{integrity`, so `has_integrity` went
/// false and `origin` went `Registry` → `Elsewhere` — and `rules::slopsquat`
/// skips every `Elsewhere` package by design, because a package fetched from
/// git never passed through the registry the corpus samples. Inline: two
/// HALLUCINATION RISK findings. Reformatted: none, and no error either. A
/// detection tool that stops detecting when someone runs the file through a
/// YAML formatter is worse than one that never looked.
#[test]
fn the_two_spellings_of_a_lockfile_agree() {
    let inline = "\
lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      lodahs:
        specifier: ^1.0.0
        version: 1.0.0
      raect:
        specifier: ^1.0.0
        version: 1.0.0
      chalk:
        specifier: ^5.0.0
        version: 5.0.0

packages:

  lodahs@1.0.0:
    resolution: {integrity: sha512-AAAA}
  raect@1.0.0:
    resolution: {integrity: sha512-BBBB}
  chalk@5.0.0:
    resolution: {integrity: sha512-CCCC}
";
    let own_line = reformatted(inline);
    assert!(own_line.contains("resolution:\n      {integrity: sha512-AAAA}"));

    let a = read(inline).expect("inline");
    let b = read(&own_line).expect("own-line");

    let findings = slopsquat_findings(&a);
    // Two invented names against a corpus of three real ones. An equivalence
    // between two empty lists would prove nothing.
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert_eq!(findings, slopsquat_findings(&b));

    // And the fields the rule reached through to get there.
    assert!(a.packages.iter().all(|p| p.has_integrity));
    assert!(b.packages.iter().all(|p| p.has_integrity));
    for (x, y) in a.packages.iter().zip(&b.packages) {
        assert_eq!((&x.key, x.origin), (&y.key, y.origin));
    }
}

/// The same reformat, on the real 254 KB file: 1,698 lines move their brace
/// down one — 850 `resolution:`, 406 `engines:`, 128 `os:`/`cpu:` and every
/// `{}` snapshot. The tree that comes out has to be the same tree.
#[test]
fn the_reformatted_fixture_is_the_same_tree() {
    let plain = load();
    let src = fs::read_to_string(path(FIXTURE)).unwrap();
    let moved_src = reformatted(&src);
    // A `reformatted` that quietly did nothing would pass everything below.
    assert_eq!(moved_src.lines().count() - src.lines().count(), 1_698);
    let moved = pnpm::read(&path(FIXTURE), &moved_src).expect("reformatted fixture");

    assert_eq!(moved.packages.len(), 850);
    assert_eq!(moved.edges, plain.edges);
    assert_eq!(moved.roots, plain.roots);
    assert_eq!(
        moved.packages.iter().filter(|p| p.has_integrity).count(),
        850
    );
    assert_eq!(
        moved
            .packages
            .iter()
            .filter(|p| p.origin == Origin::Registry)
            .count(),
        850
    );
    for (x, y) in plain.packages.iter().zip(&moved.packages) {
        assert_eq!(x.key, y.key);
    }
}

/// All 850 resolutions are `{integrity: …}` with nothing else in them, which
/// is pnpm's shape for a registry tarball. A `tarball`, `repo` or `directory`
/// key would mean the corpus has nothing useful to say about the name.
#[test]
fn origin_comes_from_the_resolution_shape() {
    let t = load();
    assert_eq!(
        t.packages
            .iter()
            .filter(|p| p.origin == Origin::Registry)
            .count(),
        850
    );

    let src = "\
lockfileVersion: '9.0'

packages:

  reg@1.0.0:
    resolution: {integrity: sha512-rrr==}
  tar@1.0.0:
    resolution: {tarball: https://example.invalid/tar-1.0.0.tgz}
  git@1.0.0:
    resolution: {type: git, repo: git@example.invalid:x.git, commit: abc}
  local@1.0.0:
    resolution: {type: directory, directory: ../local}
";
    let t = read(src).expect("should read");
    assert_eq!(find(&t, "reg").origin, Origin::Registry);
    for name in ["tar", "git", "local"] {
        assert_eq!(find(&t, name).origin, Origin::Elsewhere, "{name}");
    }
}

/// A lockfile pins by definition. The `^5.8.3` lives in the importer's
/// `specifier`, which is not what got installed.
#[test]
fn everything_is_pinned() {
    let t = load();
    assert!(t.packages.iter().all(|p| p.pinned == Pin::Exact));
}

/// `optional: true` sits on the snapshot, not the package, and 73 snapshots
/// carry it.
#[test]
fn optional_comes_from_the_snapshot() {
    let t = load();
    assert_eq!(t.packages.iter().filter(|p| p.optional).count(), 73);
}

// -- refusals ---------------------------------------------------------------

/// Version 6 and below keep the dependency graph inside `packages` and have no
/// `snapshots` section, so this reader would find zero edges and report a tree
/// where nothing depends on anything. Refusing by name beats that.
#[test]
fn older_lockfile_versions_are_refused() {
    let err = read("lockfileVersion: '6.0'\npackages:\n  a@1: {}\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("lockfileVersion 6.0 is not supported")
    );
    let err = read("lockfileVersion: '5.4'\n").unwrap_err();
    assert!(err.to_string().contains("stranger reads 9"));
    // pnpm 10 still writes 9.0.
    assert!(read("lockfileVersion: '9.0'\npackages:\n  a@1: {}\n").is_ok());
}

#[test]
fn a_file_that_is_not_a_lockfile_is_refused() {
    let err = read("hello: world\n").unwrap_err();
    assert!(err.to_string().contains("no lockfileVersion"));
}

/// A missing `packages:` section is not one of the refusals. A project with
/// no third-party dependencies writes exactly this file, and it is legal —
/// refusing it made "you depend on nobody" indistinguishable from "your
/// lockfile is broken".
#[test]
fn a_lock_with_no_packages_is_an_empty_tree() {
    let t = read("lockfileVersion: '9.0'\nsettings: {}\n").expect("a legal v9 file");
    assert!(t.packages.is_empty());
    assert!(t.edges.is_empty());
    assert!(t.roots.is_empty());
}

/// A syntax error travels out of the reader with its position intact rather
/// than becoming "could not read lockfile".
#[test]
fn a_broken_file_reports_where() {
    match read("lockfileVersion: '9.0'\npackages:\n\ta@1: {}\n") {
        Err(Error::Syntax { line, col, what }) => {
            assert_eq!((line, col), (3, 1));
            assert_eq!(what, "tab used for indentation");
        }
        other => panic!("expected a positioned syntax error, got {other:?}"),
    }
}

// -- the graph, on a file small enough to check by hand ---------------------

/// The peer-suffix rule, end to end. Both ends of every edge carry a suffix
/// the `packages` section has never seen: the snapshot key
/// `a@1.0.0(c@3.0.0)`, and the dependency value `b: 2.0.0(c@3.0.0)`. Truncate
/// neither and the graph is empty.
#[test]
fn peer_suffixes_resolve_to_the_base_package() {
    let src = "\
lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      a:
        specifier: ^1.0.0
        version: 1.0.0(c@3.0.0)

packages:

  a@1.0.0:
    resolution: {integrity: sha512-aaa==}
  b@2.0.0:
    resolution: {integrity: sha512-bbb==}
  c@3.0.0:
    resolution: {integrity: sha512-ccc==}

snapshots:

  a@1.0.0(c@3.0.0):
    dependencies:
      b: 2.0.0(c@3.0.0)
  b@2.0.0(c@3.0.0):
    dependencies:
      c: 3.0.0
  c@3.0.0: {}
";
    let t = read(src).expect("should read");
    assert_eq!(t.packages.len(), 3);
    assert_eq!(t.direct(), 1);
    assert_eq!(degree_of(&t, "b"), 1);
    assert_eq!(degree_of(&t, "c"), 1);
    assert_eq!(degree_of(&t, "a"), 0);
    // `a` came in through the importer, so it is a root and not an edge.
    assert_eq!(t.edges.len(), 2);
}

/// A workspace sibling is `link:../shared` and has no `packages` entry. It
/// must not become a root pointing at whatever happens to sort nearby.
#[test]
fn link_dependencies_resolve_to_nothing() {
    let src = "\
lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      shared:
        specifier: workspace:*
        version: link:../shared
      real:
        specifier: ^1.0.0
        version: 1.0.0

  ../shared: {}

packages:

  real@1.0.0:
    resolution: {integrity: sha512-rrr==}
";
    let t = read(src).expect("should read");
    assert_eq!(t.packages.len(), 1);
    assert_eq!(t.direct(), 1);
    assert_eq!(t.packages[t.roots[0]].name, "real");
}

/// Every workspace member's manifest is the thing under audit, so its
/// dependencies are roots too — not edges out of the member.
#[test]
fn every_importer_contributes_roots() {
    let src = "\
lockfileVersion: '9.0'

importers:

  .:
    devDependencies:
      one:
        specifier: ^1.0.0
        version: 1.0.0

  apps/web:
    dependencies:
      two:
        specifier: ^2.0.0
        version: 2.0.0

packages:

  one@1.0.0:
    resolution: {integrity: sha512-111==}
  two@2.0.0:
    resolution: {integrity: sha512-222==}
";
    let t = read(src).expect("should read");
    assert_eq!(t.direct(), 2);
    assert!(t.edges.is_empty());
}
