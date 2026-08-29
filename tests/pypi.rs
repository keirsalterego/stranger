//! poetry.lock and uv.lock.
//!
//! The counts here were measured against the files — `grep -c '^\[\[package\]\]'`
//! for packages, the reader itself for edges — and not copied from anywhere.
//! Edge counts are asserted exactly rather than as `> 0` because the whole
//! point of these two readers is the graph, and a graph that silently loses a
//! tenth of its edges still passes a non-empty check.

use std::fs;
use std::path::{Path, PathBuf};
use stranger::corpus;
use stranger::lock::{Ecosystem, Pin, Tree, pypi};
use stranger::rules::slopsquat::{self, Config};

fn path_to(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn load(name: &str) -> Tree {
    stranger::lock::read(&path_to(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn degree_of(t: &Tree, name: &str) -> u32 {
    let deg = t.in_degree();
    let i = t
        .packages
        .iter()
        .position(|p| p.name == name)
        .unwrap_or_else(|| panic!("no {name}"));
    deg[i]
}

fn has_edge(t: &Tree, from: &str, to: &str) -> bool {
    let find = |n: &str| t.packages.iter().position(|p| p.name == n);
    match (find(from), find(to)) {
        (Some(a), Some(b)) => t.edges.contains(&(a, b)),
        _ => false,
    }
}

#[test]
fn counts() {
    assert_eq!(load("poetry-s.poetry.lock").packages.len(), 54);
    assert_eq!(load("poetry-m.poetry.lock").packages.len(), 233);
    assert_eq!(load("uv-m.uv.lock").packages.len(), 250);
}

#[test]
fn edge_counts() {
    assert_eq!(load("poetry-s.poetry.lock").edges.len(), 62);
    assert_eq!(load("poetry-m.poetry.lock").edges.len(), 283);
    assert_eq!(load("uv-m.uv.lock").edges.len(), 476);
}

/// The reason these readers exist. `requirements.txt` cannot answer this.
#[test]
fn transitive_packages_have_parents() {
    for name in [
        "poetry-s.poetry.lock",
        "poetry-m.poetry.lock",
        "uv-m.uv.lock",
    ] {
        let t = load(name);
        assert_eq!(t.ecosystem, Ecosystem::PyPi);
        for dep in ["certifi", "charset-normalizer", "idna", "urllib3"] {
            assert!(degree_of(&t, dep) > 0, "{name}: {dep} has no parent");
            assert!(has_edge(&t, "requests", dep), "{name}: requests -> {dep}");
        }
    }
}

/// The bug this file exists to prevent.
///
/// poetry writes package names normalised and dependency keys exactly as the
/// depending project typed them, so these four edges in poetry-s only exist
/// if both sides are folded through PEP 503 first. Comparing raw strings
/// drops them silently — no error, just four packages that suddenly look like
/// nothing depends on them, which is the shape slopsquat fires on.
///
/// 4 of poetry-s's 62 edges and 27 of poetry-m's 283 need this.
#[test]
fn separator_spelling_does_not_break_a_link() {
    let s = load("poetry-s.poetry.lock");
    // key in the file          package entry it has to reach
    for (key, pkg) in [
        ("charset_normalizer", "charset-normalizer"),
        ("jaraco.classes", "jaraco-classes"),
        ("SecretStorage", "secretstorage"),
        ("pyproject_hooks", "pyproject-hooks"),
    ] {
        assert_ne!(key, pkg, "pick a case the raw comparison would get right");
        assert_eq!(
            corpus::normalize(Ecosystem::PyPi, key),
            corpus::normalize(Ecosystem::PyPi, pkg)
        );
        assert!(degree_of(&s, pkg) > 0, "{pkg} lost its edge from `{key}`");
    }

    let m = load("poetry-m.poetry.lock");
    for pkg in [
        "pyyaml",            // written `PyYAML` and `PyYaml` by two dependants
        "typing-extensions", // `typing_extensions`
        "zope-interface",    // `zope.interface`
        "boolean-py",        // `boolean.py`
        "click",             // `Click`
    ] {
        assert!(degree_of(&m, pkg) > 0, "{pkg} lost its edge");
    }
}

/// poetry.lock does not record the root's direct dependencies, so `roots` is
/// derived: exactly the entries nothing else in the file depends on.
#[test]
fn poetry_roots_are_the_in_degree_zero_set() {
    for name in ["poetry-s.poetry.lock", "poetry-m.poetry.lock"] {
        let t = load(name);
        let deg = t.in_degree();
        let derived: Vec<usize> = (0..t.packages.len()).filter(|&i| deg[i] == 0).collect();
        assert_eq!(t.roots, derived, "{name}");
        assert!(!t.roots.is_empty(), "{name}");
    }
    assert_eq!(load("poetry-s.poetry.lock").direct(), 7);
    assert_eq!(load("poetry-m.poetry.lock").direct(), 75);
}

/// uv.lock does record it, as the entry with `source = { editable = "." }`.
/// That entry is the manifest under audit, so its edges are roots and it is
/// not one of its own dependencies.
///
/// The recorded set is strictly larger than what poetry's derivation would
/// produce — 91 against 60 — because 31 of uv-m's direct dependencies are
/// also depended on by something else and so have a non-zero in-degree. That
/// gap is the price of the derivation, measured.
#[test]
fn uv_records_its_own_root() {
    let t = load("uv-m.uv.lock");
    let root = t
        .packages
        .iter()
        .position(|p| p.name == "hermes-agent")
        .expect("uv-m's editable entry");
    assert!(t.packages[root].first_party);
    assert!(!t.roots.contains(&root));
    assert_eq!(t.direct(), 91);

    let deg = t.in_degree();
    let orphans: Vec<usize> = (0..t.packages.len())
        .filter(|&i| deg[i] == 0 && !t.packages[i].first_party)
        .collect();
    assert_eq!(orphans.len(), 60);
    for i in &orphans {
        assert!(t.roots.contains(i), "{} is nobody's", t.packages[*i].name);
    }
    assert_eq!(t.roots.iter().filter(|&&i| deg[i] > 0).count(), 31);
}

/// uv writes one entry per resolution branch when a marker splits a name
/// across versions. `scipy` is the only such pair in uv-m; if the dependency
/// entries' `version` were ignored, one fork would take both edges and the
/// other would look like a package nothing depends on.
#[test]
fn scipy_forks_keep_their_own_parents() {
    let t = load("uv-m.uv.lock");
    let deg = t.in_degree();
    let forks: Vec<(&str, u32)> = t
        .packages
        .iter()
        .enumerate()
        .filter(|(_, p)| p.name == "scipy")
        .map(|(i, p)| (p.version.as_str(), deg[i]))
        .collect();
    assert_eq!(forks, vec![("1.17.1", 2), ("1.18.0", 2)]);
}

/// A lockfile entry is the resolver's answer, so every pin is exact.
#[test]
fn every_entry_is_pinned() {
    for name in [
        "poetry-s.poetry.lock",
        "poetry-m.poetry.lock",
        "uv-m.uv.lock",
    ] {
        let t = load(name);
        for p in &t.packages {
            assert_eq!(p.pinned, Pin::Exact, "{name}: {}", p.name);
            assert!(!p.version.is_empty(), "{name}: {} has no version", p.name);
        }
    }
}

/// poetry's `files = []` and uv's editable entry are the only entries in the
/// corpus with no hash, and both are honest: a git checkout and the project
/// itself have nothing for an index to have signed.
#[test]
fn integrity_is_recorded_except_where_there_is_none() {
    let without = |name: &str| -> Vec<String> {
        load(name)
            .packages
            .iter()
            .filter(|p| !p.has_integrity)
            .map(|p| p.name.clone())
            .collect()
    };
    assert_eq!(without("poetry-s.poetry.lock"), ["pyinotify"]);
    assert_eq!(without("poetry-m.poetry.lock"), ["PyKMIP"]);
    assert_eq!(without("uv-m.uv.lock"), ["hermes-agent"]);
}

/// Names go in as written. `PyKMIP` is the one entry in either poetry fixture
/// that poetry did not normalise for us, and the report has to be able to
/// quote a string the reader can find in the file.
#[test]
fn names_kept_as_written() {
    let t = load("poetry-m.poetry.lock");
    assert!(t.packages.iter().any(|p| p.name == "PyKMIP"));
    assert!(!t.packages.iter().any(|p| p.name == "pykmip"));
}

/// poetry records group membership per package; uv does not record one at
/// all, and neither format records install-time code. Pinned so the
/// limitation is a test failure if anybody invents a proxy for it.
#[test]
fn dev_and_install_script() {
    let s = load("poetry-s.poetry.lock");
    assert_eq!(s.packages.iter().filter(|p| p.dev).count(), 39);
    let m = load("poetry-m.poetry.lock");
    assert_eq!(m.packages.iter().filter(|p| p.dev).count(), 202);

    let u = load("uv-m.uv.lock");
    assert_eq!(u.packages.iter().filter(|p| p.dev).count(), 0);

    for t in [&s, &m, &u] {
        assert!(t.packages.iter().all(|p| !p.install_script));
    }
}

#[test]
fn dispatch_by_suffix() {
    assert_eq!(load("poetry-m.poetry.lock").ecosystem, Ecosystem::PyPi);
    assert_eq!(load("uv-m.uv.lock").ecosystem, Ecosystem::PyPi);
    assert!(stranger::lock::KNOWN.contains(&"poetry.lock"));
    assert!(stranger::lock::KNOWN.contains(&"uv.lock"));
}

/// A `Cargo.lock` is TOML with a top-level `version` and hundreds of
/// `[[package]]` entries, so a filename mixup would otherwise read as a clean
/// several-hundred-package Python project with every edge missing.
#[test]
fn a_cargo_lock_is_refused_by_both() {
    let src = fs::read_to_string(path_to("cargo-s.Cargo.lock")).unwrap();
    let p = Path::new("uv.lock");
    assert!(pypi::uv(p, &src).is_err());
    assert!(pypi::poetry(p, &src).is_err());
}

#[test]
fn malformed_input_does_not_panic() {
    let p = Path::new("poetry.lock");
    for src in [
        "",
        "not toml at all {{{",
        "[metadata]\nlock-version = \"2.1\"\n",
        "[metadata]\nlock-version = \"2.1\"\n[[package]]\nversion = \"1\"\n",
        "[metadata]\nlock-version = \"2.1\"\n[[package]]\nname = \"a\"\n[package.dependencies]\na = \"*\"\n",
        "requires-python = \">=3.9\"\n[[package]]\nname = \"a\"\ndependencies = [{ name = 3 }]\n",
    ] {
        let _ = pypi::poetry(p, src);
        let _ = pypi::uv(Path::new("uv.lock"), src);
    }
    // An entry that lists itself must not end up vouching for itself.
    let t = pypi::poetry(
        p,
        "[metadata]\nlock-version = \"2.1\"\n[[package]]\nname = \"a\"\nversion = \"1\"\n[package.dependencies]\nA = \"*\"\n",
    )
    .expect("parses");
    assert!(t.edges.is_empty());
    assert_eq!(t.roots, vec![0]);
}

/// An empty lock is a lock. A project with no dependencies gets a file with a
/// header and nothing under it, and refusing that would be a false alarm.
#[test]
fn empty_lockfiles_are_empty_not_wrong() {
    let t = pypi::poetry(
        Path::new("poetry.lock"),
        "[metadata]\nlock-version = \"2.1\"\ncontent-hash = \"x\"\n",
    )
    .expect("an empty poetry.lock");
    assert!(t.packages.is_empty());
    assert!(t.edges.is_empty());
    assert!(t.roots.is_empty());
}

/// The point of the whole exercise, as a number.
///
/// Clause 3 of the slopsquat rule ("nothing depends on this name") cannot do
/// anything on a `requirements.txt`, because there are no edges for it to
/// read. A complete corpus hides that — clause 1 alone eliminates every
/// candidate before clause 3 is ever consulted — so this thins the corpus the
/// way `tests/ablation.rs` thins npm's and counts what clause 3 removes at
/// each size.
///
/// The answer is 45.5–80.0% of candidates removed on the two lockfiles, and
/// exactly nothing at a complete corpus, where clause 1 leaves clause 3 no
/// candidates to work on. Anything quoting a figure for this belongs
/// downstream of this test, not beside it.
///
/// The candidate counts themselves fell by roughly two thirds when clause 2
/// got a length-relative budget — 30 down to 10 on `poetry-m` at 70% — because
/// most of what a decayed corpus hands clause 3 is short names that were near
/// something by arithmetic. That is the same effect from the other side: the
/// clause looks less impressive because it is being given less rubbish.
///
/// The share removed is a published figure, so it is pinned as exact counts
/// rather than a range. Nothing here is sampled at runtime: the fixtures are
/// checked in and the thinning is a seeded xorshift, so a moved number means a
/// reader or the rule changed and the docs are now wrong — it is not flake.
///
/// `cargo test --test pypi -- --nocapture in_degree` prints the table.
#[test]
fn the_in_degree_clause_only_works_where_there_is_a_graph() {
    const SEED: u64 = 0x5EED_1234;
    const KEEP: [u64; 5] = [1000, 900, 700, 500, 250];

    // Two files that record a graph, two that structurally cannot.
    const FILES: [&str; 4] = [
        "poetry-m.poetry.lock",
        "uv-m.uv.lock",
        "reqs-s.requirements.txt",
        "reqs-xs.requirements.txt",
    ];

    // (corpus permille, file, findings with clause 3 off, with clause 3 on).
    // Read off the table below, not predicted.
    const EXPECTED: &[(u64, &str, usize, usize)] = &[
        (1000, "poetry-m.poetry.lock", 0, 0),
        (1000, "uv-m.uv.lock", 0, 0),
        (1000, "reqs-s.requirements.txt", 0, 0),
        (1000, "reqs-xs.requirements.txt", 1, 1),
        (900, "poetry-m.poetry.lock", 3, 1),
        (900, "uv-m.uv.lock", 4, 2),
        (900, "reqs-s.requirements.txt", 0, 0),
        (900, "reqs-xs.requirements.txt", 1, 1),
        (700, "poetry-m.poetry.lock", 10, 4),
        (700, "uv-m.uv.lock", 15, 3),
        (700, "reqs-s.requirements.txt", 1, 1),
        (700, "reqs-xs.requirements.txt", 1, 1),
        (500, "poetry-m.poetry.lock", 11, 6),
        (500, "uv-m.uv.lock", 10, 2),
        (500, "reqs-s.requirements.txt", 1, 1),
        (500, "reqs-xs.requirements.txt", 0, 0),
        (250, "poetry-m.poetry.lock", 13, 5),
        (250, "uv-m.uv.lock", 8, 2),
        (250, "reqs-s.requirements.txt", 1, 1),
        (250, "reqs-xs.requirements.txt", 1, 1),
    ];

    let full = corpus::names(Ecosystem::PyPi);
    let trees: Vec<Tree> = FILES.iter().map(|&f| load(f)).collect();

    println!();
    println!("corpus decay — pypi, {} names at 100%", full.len());
    println!();
    println!("| corpus kept | file | clause 3 off | on | removed | share |");
    println!("|---|---|---|---|---|---|");

    let mut rows = Vec::new();
    for keep in KEEP {
        let subset = thinned(full, keep, SEED);
        for (file, tree) in FILES.iter().zip(&trees) {
            let count = |require_no_parent| {
                slopsquat::scan(
                    tree,
                    Config {
                        require_no_parent,
                        corpus: Some(&subset),
                    },
                )
                .len()
            };
            let (off, on) = (count(false), count(true));
            println!(
                "| {}% ({}) | {file} | {off} | {on} | {} | {} |",
                keep / 10,
                subset.len(),
                off - on,
                share(off, on).map_or("n/a".to_string(), |s| format!("{s:.1}%")),
            );
            rows.push((keep, *file, off, on));
        }
    }
    println!();

    assert_eq!(rows, EXPECTED, "the published figure moved");

    let flat = |f: &str| f.ends_with(".requirements.txt");

    // Not "happens to be zero at the sizes we tried" — a flat file records no
    // edge, so there is nothing for clause 3 to read at any size.
    for (keep, file, off, on) in &rows {
        if flat(file) {
            assert_eq!(off, on, "{file} suppressed something at {keep}permille");
        }
    }

    // The range the docs are allowed to quote. Both ends are rows above:
    // poetry at 50% is the floor, uv at 70% and 50% the ceiling. The
    // complete-corpus rows drop out because they had no candidates at all —
    // clause 1 took every one of them first, which is the honest reason
    // clause 3 looks worthless right up until the corpus starts missing names.
    let graphed: Vec<f64> = rows
        .iter()
        .filter(|(_, file, ..)| !flat(file))
        .filter_map(|&(_, _, off, on)| share(off, on))
        .collect();
    let lo = graphed.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = graphed.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(format!("{lo:.1}–{hi:.1}%"), "45.5–80.0%");
}

/// Deterministic corpus thinning, lifted from `tests/ablation.rs` so the two
/// tables decay identically and can be read side by side. Order is preserved,
/// so the subset is still sorted and the `binary_search` inside
/// `corpus::contains_in` still holds.
fn thinned(names: &[&'static str], keep_permille: u64, seed: u64) -> Vec<&'static str> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(names.len());
    for &n in names {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        if state % 1000 < keep_permille {
            out.push(n);
        }
    }
    out
}

/// The fraction of candidates clause 3 removed, or `None` where there were no
/// candidates. "Removed none of zero" is a different claim from "removed none
/// of them" and printing `0.0%` for it would read as the clause failing.
fn share(off: usize, on: usize) -> Option<f64> {
    (off > 0).then(|| (off - on) as f64 / off as f64 * 100.0)
}
