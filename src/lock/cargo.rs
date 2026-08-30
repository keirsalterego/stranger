//! Cargo.lock — an array of `[[package]]` tables and nothing else.
//!
//! Structurally this is the easiest of the three formats: no install paths to
//! reproduce, no nesting, no `dependencies` object keyed by name against a
//! range. The whole file is a flat list plus one array of strings per entry.
//! What makes it non-trivial is that those strings are not names.
//!
//! # The three shapes
//!
//! Cargo writes a dependency in the shortest form that is unambiguous, and
//! promotes only when it has to:
//!
//! ```text
//! "bytes"                                  name
//! "winapi 0.3.9"                           name version
//! "qux 1.0.0 (registry+https://…)"         name version source
//! ```
//!
//! The second form appears when two entries share a name — `cargo-m` has five
//! `hashbrown`s and three `windows-sys`. The third appears when two entries
//! share a name *and* a version and differ only in where they came from.
//!
//! Counted across the three fixtures (259 + 2,223 + 3,207 = 5,689 dependency
//! strings in total):
//!
//! | fixture | bare | name+version | name+version+source |
//! |---|---|---|---|
//! | `cargo-s` | 251 | 8 | 0 |
//! | `cargo-m` | 1,723 | 500 | 0 |
//! | `cargo-l` | 2,610 | 597 | 0 |
//!
//! So the third shape is **not exercised by any real fixture here**. It is
//! implemented and tested against a hand-written file, and that is the honest
//! status: handled, unmeasured. The second shape is very much exercised —
//! 1,105 strings, a fifth of the corpus — and reading it as a bare name would
//! resolve half the `windows-sys` edges in `cargo-m` to the wrong entry.
//!
//! The invariant that makes the bare form safe is Cargo's, not ours: a bare
//! name is only written when exactly one entry carries it. That was checked,
//! not assumed — across all three fixtures, zero bare names refer to a
//! duplicated package, and zero dependency strings of any shape fail to
//! resolve.
//!
//! # No `source` means somebody in this repo wrote it
//!
//! A registry crate has `source = "registry+https://…"`; a git dependency has
//! `source = "git+https://…#rev"`. A workspace member or a `path = "…"`
//! dependency has no `source` key at all, because there is nowhere to fetch it
//! from. That is the whole test, and it is npm's `link: true` / bare workspace
//! directory rule wearing different clothes.
//!
//! It matters for the same reason: an edge out of a first-party package is a
//! manifest in the repo under audit, not independent evidence that a name is
//! real. `cargo-l` is a 944-entry workspace with 93 members; if their edges
//! counted, a hallucinated crate added to any one of those 93 `Cargo.toml`s
//! would arrive with in-degree 1 and never be looked at. So those edges go to
//! `roots`, and `roots` excludes first-party entries themselves.
//!
//! A git dependency is *not* first-party. Somebody outside this repo wrote it,
//! and the fact that it bypasses crates.io is if anything more interesting.
//!
//! # What this file does not record, and what that costs
//!
//! - **dev-dependencies.** Cargo.lock does not distinguish them. It does not
//!   distinguish optional ones either — a feature-gated dependency that was
//!   resolved is written exactly like any other. Both `dev` and `optional` are
//!   therefore `false` on every package, and that is a limitation, not a
//!   measurement. A reader that reported `dev: false` while implying it had
//!   checked would be lying. Splitting them out needs `Cargo.toml`, and the
//!   workspace's, and feature unification, which is a resolver.
//! - **build scripts.** Cargo runs `build.rs` at compile time — the same
//!   arbitrary-code-execution shape npm's `hasInstallScript` flags — but the
//!   lockfile records nothing about it. `install_script` is `false` on every
//!   package. There is no proxy for it in this file, and inventing one (the
//!   `-sys` suffix, say) would produce a confident wrong answer, which is
//!   worse than a blank. It needs the crate's `.crate` archive or the index's
//!   metadata, both of which mean fetching.
//! - **checksums for pre-v2 lockfiles.** `checksum` on the package table is
//!   v2-and-later. Cargo v1 kept them in a `[metadata]` table keyed
//!   `"checksum bytes 1.0.0 (registry+…)"`. This reader does not look there,
//!   so a v1 file reads as having no integrity anywhere.
//!   // No v1 file in the corpus, and cargo has rewritten them on
//!   every `cargo update` since 2019. Read the `[metadata]` keys if one ever
//!   turns up.
//!
//! `has_integrity` is otherwise exact: 93 of `cargo-l`'s 944 entries have no
//! checksum and all 93 are the workspace members. `cargo-m` has 34 without
//! one — 15 workspace members and 19 git dependencies, which have a source and
//! no checksum, because a git revision is its own integrity claim.

use crate::error::{Error, Result};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use crate::toml::{self, Value};
use std::collections::HashMap;
use std::path::Path;

pub fn read(path: &Path, src: &str) -> Result<Tree> {
    let doc = toml::parse(src)?;

    let entries = doc
        .get("package")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            Error::usage(format!(
                "{}: no [[package]] entries; this does not look like a Cargo.lock",
                path.display()
            ))
        })?;

    let mut packages = Vec::with_capacity(entries.len());
    // Parallel to `packages`, because `Package` has no source field and the
    // three-token dependency form needs one to disambiguate against.
    let mut sources: Vec<Option<&str>> = Vec::with_capacity(entries.len());
    let mut index: HashMap<&str, Vec<usize>> = HashMap::new();

    for (i, entry) in entries.iter().enumerate() {
        let field = |k: &str| entry.get(k).and_then(Value::as_str);
        // A position would be better than an ordinal, but the value tree does
        // not carry one. `[[package]] #17` is still something you can count to.
        let missing = |k: &str| {
            Error::usage(format!(
                "{}: [[package]] #{} has no `{k}`",
                path.display(),
                i + 1
            ))
        };
        let name = field("name").ok_or_else(|| missing("name"))?;
        let version = field("version").ok_or_else(|| missing("version"))?;
        let source = field("source");

        index.entry(name).or_default().push(i);
        sources.push(source);
        packages.push(Package {
            name: name.to_string(),
            version: version.to_string(),
            // Not a path key — Cargo.lock has none. `name version` rather than
            // `name` alone because names repeat (five `hashbrown`s in
            // `cargo-m`), and this is exactly the string Cargo itself writes
            // when it has to disambiguate one. Unique across all three
            // fixtures; `name` alone is not.
            key: format!("{name} {version}"),
            dev: false,
            optional: false,
            first_party: source.is_none(),
            install_script: false,
            has_integrity: entry.get("checksum").is_some(),
            // 689 registry against 19 git in cargo-m. The git ones are why
            // this field exists: `slint` and `sg` are real crates that never
            // went through crates.io, so a crates.io corpus cannot have heard
            // of them and their absence proves nothing.
            origin: match entry.get("source").and_then(Value::as_str) {
                Some(src) if src.starts_with("registry+") => Origin::Registry,
                _ => Origin::Elsewhere,
            },
            pinned: Pin::Exact,
        });
    }

    let mut edges = Vec::new();
    let mut roots = Vec::new();

    for (from, entry) in entries.iter().enumerate() {
        let Some(deps) = entry.get("dependencies").and_then(Value::as_array) else {
            continue;
        };
        let manifest_we_are_auditing = packages[from].first_party;
        for dep in deps {
            let Some(dep) = dep.as_str() else { continue };
            // A dependency naming an entry that is not in the file means the
            // lockfile is corrupt. Skipping the edge rather than refusing the
            // file keeps the other 3,206 edges readable, and it fails in the
            // safe direction: a missing edge lowers in-degree, and low
            // in-degree makes the slopsquat rule *more* likely to speak up.
            // Erroring out trades a whole scan for one bad string.
            let Some(to) = resolve(&index, &packages, &sources, dep) else {
                continue;
            };
            if manifest_we_are_auditing {
                roots.push(to);
            } else {
                edges.push((from, to));
            }
        }
    }

    roots.sort_unstable();
    roots.dedup();
    roots.retain(|&i| !packages[i].first_party);

    Ok(Tree {
        ecosystem: Ecosystem::Crates,
        source: path.to_path_buf(),
        packages,
        edges,
        roots,
        records_edges: true,
        records_install_scripts: false,
    })
}

/// One dependency string to one package index.
///
/// The three shapes narrow in order: the name picks the candidates, a version
/// picks among them, a source breaks a tie the version could not.
fn resolve(
    index: &HashMap<&str, Vec<usize>>,
    packages: &[Package],
    sources: &[Option<&str>],
    dep: &str,
) -> Option<usize> {
    let (name, want_version, want_source) = split(dep);
    let candidates = index.get(name)?;

    let Some(want_version) = want_version else {
        // Cargo only writes the bare form when the name is unique, so this is
        // a one-element list in every fixture. Taking the first if that ever
        // stops being true beats dropping the edge — see the corruption note
        // above — and both candidates share the name the rules key on anyway.
        return candidates.first().copied();
    };

    let mut fallback = None;
    for &i in candidates {
        if packages[i].version != want_version {
            continue;
        }
        if want_source.is_some_and(|s| sources[i] == Some(s)) {
            return Some(i);
        }
        fallback.get_or_insert(i);
    }
    // Reached when no source was given, or when one was and matched nothing —
    // a source we cannot match is still a version we can.
    fallback
}

/// `"qux 1.0.0 (registry+https://…)"` into its three parts.
///
/// Whitespace-split rather than a parser because none of the three fields can
/// contain a space: crate names are `[A-Za-z0-9_-]`, semver has none, and a
/// source is a URL with an optional `#rev`. A trailing token that is not
/// parenthesised is dropped rather than treated as a source, which leaves the
/// name and version — the two parts that decide the edge — still doing the work.
fn split(dep: &str) -> (&str, Option<&str>, Option<&str>) {
    let dep = dep.trim();
    let Some((name, rest)) = dep.split_once(char::is_whitespace) else {
        return (dep, None, None);
    };
    let rest = rest.trim_start();
    let Some((version, rest)) = rest.split_once(char::is_whitespace) else {
        return (name, Some(rest), None);
    };
    let source = rest
        .trim()
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'));
    (name, Some(version), source)
}
