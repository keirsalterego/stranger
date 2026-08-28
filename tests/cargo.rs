use std::fs;
use std::path::{Path, PathBuf};
use stranger::lock::cargo;
use stranger::lock::{Ecosystem, Package, Tree};

fn path_to(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn load(name: &str) -> Tree {
    let path = path_to(name);
    let src = fs::read_to_string(&path).unwrap();
    cargo::read(&path, &src).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Hand-written input, for the shapes the real fixtures do not contain.
fn parse(src: &str) -> Tree {
    cargo::read(Path::new("Cargo.lock"), src).unwrap_or_else(|e| panic!("{e}"))
}

fn find<'a>(t: &'a Tree, name: &str) -> &'a Package {
    t.packages
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn index_of(t: &Tree, name: &str, version: &str) -> usize {
    t.packages
        .iter()
        .position(|p| p.name == name && p.version == version)
        .unwrap_or_else(|| panic!("no {name} {version}"))
}

fn degree_of(t: &Tree, name: &str) -> u32 {
    let deg = t.in_degree();
    t.packages
        .iter()
        .enumerate()
        .filter(|(_, p)| p.name == name)
        .map(|(i, _)| deg[i])
        .sum()
}

/// `grep -c '^\[\[package\]\]'`, not anyone's notes.
#[test]
fn counts() {
    assert_eq!(load("cargo-s.Cargo.lock").packages.len(), 124);
    assert_eq!(load("cargo-m.Cargo.lock").packages.len(), 723);
    assert_eq!(load("cargo-l.Cargo.lock").packages.len(), 944);
}

/// No `source` key, no upstream. `cargo-l` is a 93-member workspace; `cargo-s`
/// is one crate that happens to be the thing being audited.
#[test]
fn workspace_members_are_first_party() {
    let l = load("cargo-l.Cargo.lock");
    assert_eq!(l.packages.iter().filter(|p| p.first_party).count(), 93);
    assert!(find(&l, "action-example-client").first_party);
    assert!(!find(&l, "serde").first_party);

    let s = load("cargo-s.Cargo.lock");
    let own: Vec<_> = s.packages.iter().filter(|p| p.first_party).collect();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].name, "graph-greener");
}

#[test]
fn roots_exclude_first_party() {
    for name in [
        "cargo-s.Cargo.lock",
        "cargo-m.Cargo.lock",
        "cargo-l.Cargo.lock",
    ] {
        let t = load(name);
        assert!(!t.roots.is_empty(), "{name}");
        for &r in &t.roots {
            assert!(!t.packages[r].first_party, "{name}: {}", t.packages[r].key);
        }
    }
}

/// Shape one: a bare name, which cargo writes only when it is unique.
#[test]
fn bare_names_resolve() {
    let t = load("cargo-s.Cargo.lock");
    let from = index_of(&t, "aho-corasick", "1.1.4");
    let to = index_of(&t, "memchr", "2.8.2");
    assert!(t.edges.contains(&(from, to)));
}

/// Shape two, and the reason the reader cannot stop at the first token.
/// `cargo-s` holds two `anstream`s; `clap_builder` wants 0.6.21 and
/// `env_logger` wants 1.0.0. Read as bare names, both land on whichever entry
/// came first and one of these assertions fails.
#[test]
fn versioned_names_pick_the_right_twin() {
    let t = load("cargo-s.Cargo.lock");
    let old = index_of(&t, "anstream", "0.6.21");
    let new = index_of(&t, "anstream", "1.0.0");
    assert!(
        t.edges
            .contains(&(index_of(&t, "clap_builder", "4.5.60"), old))
    );
    assert!(
        t.edges
            .contains(&(index_of(&t, "env_logger", "0.11.11"), new))
    );
}

/// Shape three appears zero times in all three fixtures, so it gets a
/// hand-written file rather than a claim. Two entries, same name, same
/// version, different registries — only the parenthesised source separates
/// them.
#[test]
fn sourced_names_pick_the_right_registry() {
    let t = parse(
        r#"
version = 4

[[package]]
name = "left"
version = "0.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aa"
dependencies = ["twin 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"]

[[package]]
name = "right"
version = "0.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bb"
dependencies = ["twin 1.0.0 (git+https://example.invalid/twin.git#dead)"]

[[package]]
name = "twin"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "cc"

[[package]]
name = "twin"
version = "1.0.0"
source = "git+https://example.invalid/twin.git#dead"
"#,
    );
    assert_eq!(t.edges, vec![(0, 2), (1, 3)]);
    // The git twin is still a stranger — it just has no checksum to show.
    assert!(!t.packages[3].first_party);
    assert!(t.packages[2].has_integrity);
    assert!(!t.packages[3].has_integrity);
}

/// The clause the slopsquat rule leans on. If edge resolution were broken
/// these would be zero and every crate in the tree would look root-only.
#[test]
fn in_degree_is_not_flat() {
    assert_eq!(degree_of(&load("cargo-s.Cargo.lock"), "libc"), 11);
    assert_eq!(degree_of(&load("cargo-m.Cargo.lock"), "libc"), 67);
    let l = load("cargo-l.Cargo.lock");
    assert_eq!(degree_of(&l, "libc"), 87);
    assert_eq!(degree_of(&l, "serde"), 68);
}

/// A workspace member's dependencies are its own manifest, so they are direct
/// dependencies and not evidence. `cargo-l` would report 2,549 edges and 123
/// roots; counting the 93 members' edges as real would add 658 of them.
#[test]
fn member_edges_become_roots() {
    let t = load("cargo-l.Cargo.lock");
    assert_eq!(t.edges.len(), 2549);
    assert_eq!(t.roots.len(), 123);
    for &(from, _) in &t.edges {
        assert!(!t.packages[from].first_party);
    }
    // `arrow` is named by workspace members and by nothing else, so it is a
    // direct dependency with no parent — which is exactly the shape the
    // slopsquat rule looks at, and why getting this split right matters.
    let arrow = index_of(&t, "arrow", "59.0.0");
    assert!(t.roots.contains(&arrow));
    assert_eq!(t.in_degree()[arrow], 0);
}

/// Cargo.lock records neither, and the reader says so rather than reporting a
/// clean `false` it never checked. This test exists so the claim in the module
/// doc cannot quietly stop being true.
#[test]
fn nothing_claims_dev_or_build_scripts() {
    let t = load("cargo-m.Cargo.lock");
    assert!(!t.packages.iter().any(|p| p.dev || p.optional));
    assert!(!t.packages.iter().any(|p| p.install_script));
}

#[test]
fn checksums_are_registry_only() {
    let m = load("cargo-m.Cargo.lock");
    // 723 - 15 workspace members - 19 git dependencies.
    assert_eq!(m.packages.iter().filter(|p| p.has_integrity).count(), 689);
    let git = m
        .packages
        .iter()
        .filter(|p| !p.first_party && !p.has_integrity)
        .count();
    assert_eq!(git, 19);
}

/// A dependency naming an entry that is not in the file. Skip the edge, keep
/// the rest of the tree — a corrupt string should not cost a whole scan, and
/// the missing edge errs toward reporting rather than toward silence.
#[test]
fn a_dangling_dependency_is_skipped() {
    let t = parse(
        r#"
version = 4

[[package]]
name = "real"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aa"
dependencies = ["ghost", "other 9.9.9", "alsoreal"]

[[package]]
name = "alsoreal"
version = "2.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bb"
"#,
    );
    assert_eq!(t.packages.len(), 2);
    assert_eq!(t.edges, vec![(0, 1)]);
}

#[test]
fn a_toml_file_that_is_not_a_lockfile_is_refused() {
    let p = Path::new("Cargo.lock");
    assert!(cargo::read(p, "version = 4\n").is_err());
    assert!(cargo::read(p, "[package]\nname = \"x\"\n").is_err());
    let err = cargo::read(p, "[[package]]\nversion = \"1.0.0\"\n").unwrap_err();
    assert!(err.to_string().contains("has no `name`"), "{err}");
}

/// Fixtures are named `cargo-l.Cargo.lock`, so dispatch has to match on the
/// suffix and not be shadowed by the arm above it.
#[test]
fn dispatch_matches_the_suffix() {
    let t = stranger::lock::read(&path_to("cargo-l.Cargo.lock")).unwrap();
    assert_eq!(t.ecosystem, Ecosystem::Crates);
    assert_eq!(t.packages.len(), 944);
}

/// Git dependencies are real packages that never went through crates.io, so a
/// crates.io corpus cannot have heard of them. `slint` and `sg` in `cargo-m`
/// are exactly that, and before `Origin` existed all three slopsquat clauses
/// fired on both.
#[test]
fn git_dependencies_are_not_registry_packages() {
    let t = load("cargo-m.Cargo.lock");
    let git: Vec<&str> = t
        .packages
        .iter()
        .filter(|p| p.origin == stranger::lock::Origin::Elsewhere && !p.first_party)
        .map(|p| p.name.as_str())
        .collect();
    assert!(git.contains(&"slint"), "{git:?}");
    assert!(git.contains(&"sg"), "{git:?}");

    let found: Vec<String> = stranger::rules::slopsquat::scan(&t, Default::default())
        .into_iter()
        .map(|f| f.package)
        .collect();
    for name in &git {
        assert!(
            !found.contains(&name.to_string()),
            "{name} came from git, the corpus never covered it"
        );
    }
}
