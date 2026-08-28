use std::fs;
use std::path::{Path, PathBuf};
use stranger::lock::Tree;
use stranger::lock::npm;

fn load(name: &str) -> Tree {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let src = fs::read_to_string(&path).unwrap();
    npm::read(&path, &src).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn find<'a>(t: &'a Tree, name: &str) -> &'a stranger::lock::Package {
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

#[test]
fn counts() {
    assert_eq!(load("npm-xs.package-lock.json").packages.len(), 37);
    assert_eq!(load("npm-xl.package-lock.json").packages.len(), 1390);
}

/// The root project's own `hasInstallScript` is not a supply-chain finding —
/// it is your build. `jq` counts 9 of these in npm-xl and the reader reports
/// 8; the missing one is the root entry, deliberately.
#[test]
fn install_scripts_exclude_the_root_project() {
    let t = load("npm-xl.package-lock.json");
    assert_eq!(t.packages.iter().filter(|p| p.install_script).count(), 8);
    assert!(find(&t, "esbuild").install_script);
    assert!(!find(&t, "semver").install_script);
}

/// 184 of npm-xl's entries live at `node_modules/a/node_modules/b`. If the
/// walk-up were wrong these would resolve to the wrong entry or to nothing,
/// and the graph would be quietly garbage.
#[test]
fn nested_entries_keep_their_own_identity() {
    let t = load("npm-xl.package-lock.json");
    let nested: Vec<_> = t
        .packages
        .iter()
        .filter(|p| p.key.matches("node_modules/").count() > 1)
        .collect();
    assert_eq!(nested.len(), 184);
    // Every nested entry's name is the segment after the *last* node_modules.
    for p in &nested {
        assert!(
            p.key.ends_with(&format!("node_modules/{}", p.name)),
            "{}",
            p.key
        );
    }
    // And a scoped nested package keeps its scope.
    assert!(nested.iter().any(|p| p.name.starts_with('@')));
}

#[test]
fn workspace_members_are_first_party() {
    let t = load("npm-xl.package-lock.json");
    assert_eq!(t.packages.iter().filter(|p| p.first_party).count(), 14);
    // No first-party package is ever reported as a direct dependency of itself.
    for &r in &t.roots {
        assert!(!t.packages[r].first_party);
    }
}

/// The monorepo case. Both of these declare `workspaces` and keep next to
/// nothing in the root manifest, so reading only the root entry reports zero
/// direct dependencies for a 582-package project.
#[test]
fn workspace_deps_count_as_direct() {
    assert_eq!(load("npm-m.package-lock.json").roots.len(), 20);
    assert_eq!(load("npm-xl.package-lock.json").roots.len(), 150);
}

/// The whole point. All three planted names are root-only with nothing
/// depending on them, and the real `express` sitting beside the fake `expres`
/// is not.
#[test]
fn planted_names_have_no_parent() {
    let t = load("poisoned.package-lock.json");
    for fake in ["expres", "lodahs", "chalck"] {
        assert_eq!(degree_of(&t, fake), 0, "{fake} should have no parent");
        let i = t.packages.iter().position(|p| p.name == fake).unwrap();
        assert!(t.roots.contains(&i), "{fake} should be a root dependency");
    }
    assert!(
        degree_of(&t, "express") > 0,
        "the real express is depended on"
    );
}

#[test]
fn poisoning_added_exactly_three_entries() {
    let clean = load("npm-l.package-lock.json");
    let dirty = load("poisoned.package-lock.json");
    assert_eq!(dirty.packages.len(), clean.packages.len() + 3);
    assert_eq!(dirty.roots.len(), clean.roots.len() + 3);
    // and changed no edges, because nothing depends on a hallucination
    assert_eq!(dirty.edges.len(), clean.edges.len());
}

#[test]
fn version_one_lockfiles_are_refused_by_name() {
    let path = Path::new("v1.json");
    let err = npm::read(path, r#"{"lockfileVersion":1,"dependencies":{}}"#).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("lockfileVersion 1 is not supported"), "{msg}");
    assert!(msg.contains("npm 7"), "should say how to fix it: {msg}");
}

#[test]
fn a_json_file_that_is_not_a_lockfile_is_refused() {
    let path = Path::new("x.json");
    assert!(npm::read(path, r#"{"hello":"world"}"#).is_err());
    assert!(npm::read(path, r#"{"lockfileVersion":3}"#).is_err());
}
