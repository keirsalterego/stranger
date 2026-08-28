use std::fs;
use std::path::{Path, PathBuf};
use stranger::error::Error;
use stranger::lock::{Ecosystem, Origin, Package, Pin, Tree, pnpm};

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
    let err = read("lockfileVersion: '9.0'\nsettings: {}\n").unwrap_err();
    assert!(err.to_string().contains("no `packages` map"));
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
