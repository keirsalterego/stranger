//! pnpm-lock.yaml, lockfileVersion 9.
//!
//! Three sections carry everything this reader needs, and they are not
//! interchangeable:
//!
//! - `importers` is the project's own manifests, one per workspace directory
//!   (`.` for a single-package repo). Each entry lists `dependencies`,
//!   `devDependencies` and `optionalDependencies` as `name: {specifier,
//!   version}` — the range the human wrote and the version pnpm picked.
//! - `packages` is the 850 distinct tarballs, keyed `name@version`, carrying
//!   `resolution`, `engines`, `hasBin`, `peerDependencies` and `deprecated`.
//! - `snapshots` is the *installed instances*, keyed `name@version` plus a
//!   parenthesised peer suffix, carrying the resolved `dependencies` and
//!   `optionalDependencies`. This is where the edges are.
//!
//! # Peer suffixes
//!
//! The same tarball can be installed more than once with different peers
//! resolved, so a snapshot key is `astro@5.7.10(@types/node@22.15.3)(jiti@…)`
//! while its `packages` entry is plain `astro@5.7.10`. Dependency *values*
//! carry the suffix too — `'@volar/kit': 2.4.23(typescript@5.8.3)`. Both ends
//! of every edge therefore get truncated at the first `(` before lookup.
//!
//! Doing that with `split('@')` instead is how a naive reader loses every
//! scoped package: `@babel/core@7.27.1` has to split at the *last* `@`, and
//! `@types/node@22.15.3` inside a suffix must not be split at all.
//!
//! # What is not recorded, and is therefore not reported
//!
//! - **Install scripts.** lockfileVersion 6 had `requiresBuild`; 9 dropped it,
//!   and this file has none. `hasBin: true` appears 42 times and is *not* a
//!   substitute — it means the package ships a `bin` entry to symlink, which
//!   is not code running at install time. Mapping one to the other would put
//!   42 High findings in the report, all of them invented. `install_script`
//!   is false for every package here, and `rules::scripts` correctly says
//!   nothing about this tree.
//! - **Dev-only packages.** pnpm 9 records dev-ness on the importer's
//!   manifest, not on the package, and does not mark the transitive closure.
//!   `dev` is false throughout rather than guessed at with a graph walk.
//!   Nothing in `rules` reads the field.
//! - **First-party packages.** There are none by construction: a workspace
//!   member lives in `importers` and never in `packages`, and pnpm writes its
//!   dependents' references to it as `link:../name`, which resolves to no
//!   package entry. That is why `Package::first_party` is false everywhere
//!   here while the npm reader has to work for it.

use crate::error::{Error, Result};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use crate::yaml::{self, Value};
use std::collections::HashMap;
use std::path::Path;

pub fn read(path: &Path, src: &str) -> Result<Tree> {
    let doc = yaml::parse(src)?;

    // pnpm quotes the version, so it arrives as the string "9.0" — and this
    // parser does not turn that into a float, which is the point. Refusing an
    // older layout by name beats mis-reading it: 6 and below have no
    // `snapshots` section at all, so this reader would find no edges and
    // report a tree where nothing depends on anything.
    match doc.get("lockfileVersion").and_then(Value::as_str) {
        Some(v) if v.split('.').next() == Some("9") => {}
        Some(v) => {
            return Err(Error::usage(format!(
                "{}: lockfileVersion {v} is not supported; stranger reads 9. \
                 Run `pnpm install` with pnpm 9 or newer to upgrade the file.",
                path.display()
            )));
        }
        None => {
            return Err(Error::usage(format!(
                "{}: no lockfileVersion field; this does not look like a pnpm-lock.yaml",
                path.display()
            )));
        }
    }

    let entries = doc
        .get("packages")
        .and_then(Value::as_mapping)
        .ok_or_else(|| Error::usage(format!("{}: no `packages` map", path.display())))?;

    let mut packages = Vec::with_capacity(entries.len());
    let mut index: HashMap<&str, usize> = HashMap::with_capacity(entries.len());

    for (key, entry) in entries {
        let (name, version) = split_key(key);
        index.insert(key.as_str(), packages.len());
        packages.push(Package {
            name: name.to_string(),
            version: version.to_string(),
            key: key.clone(),
            dev: false,
            optional: false,
            first_party: false,
            install_script: false,
            has_integrity: entry
                .get("resolution")
                .and_then(|r| r.get("integrity"))
                .is_some(),
            origin: origin_of(entry),
            // A pnpm lockfile entry is the resolver's answer. The `^`s live in
            // the importer's `specifier` field, which is not what gets
            // installed.
            pinned: Pin::Exact,
        });
    }

    // `optional` is per *instance*, not per tarball: pnpm marks the snapshot
    // that is only reachable through an optionalDependencies edge. With one
    // snapshot per package in this file the distinction never bites, but a
    // tree that installs the same version both ways would set the flag from
    // whichever snapshot claimed it, which is the conservative direction.
    let snapshots = doc.get("snapshots").and_then(Value::as_mapping);
    let mut edges = Vec::new();

    for (key, snap) in snapshots.into_iter().flatten() {
        let Some(&from) = index.get(base_of(key)) else {
            continue;
        };
        if snap.get("optional").and_then(Value::as_bool) == Some(true) {
            packages[from].optional = true;
        }
        for (name, version) in resolved_deps(snap) {
            // ponytail: one allocation per edge, 1,851 of them on this file.
            // Invisible next to the parse. A borrowed two-part key is the
            // upgrade if it ever shows up in a profile.
            let want = format!("{name}@{version}");
            if let Some(&to) = index.get(base_of(&want)) {
                edges.push((from, to));
            }
        }
    }

    // A dependency edge is evidence that a package is real only when a
    // stranger wrote it. The importers are the manifests under audit — the
    // root one and every workspace member — so their dependencies are roots,
    // not edges. This is the same call the npm reader makes for `workspaces`
    // members, and for the same reason: a hallucinated name added to
    // `apps/web/package.json` would otherwise arrive with an in-edge and
    // never be looked at again.
    let mut roots = Vec::new();
    for (_dir, importer) in doc
        .get("importers")
        .and_then(Value::as_mapping)
        .into_iter()
        .flatten()
    {
        for (name, version) in declared_deps(importer) {
            // A workspace sibling is written `link:../name` and has no
            // `packages` entry. Nothing to point at, and nothing to report.
            let want = format!("{name}@{version}");
            if let Some(&to) = index.get(base_of(&want)) {
                roots.push(to);
            }
        }
    }

    edges.sort_unstable();
    edges.dedup();
    roots.sort_unstable();
    roots.dedup();

    Ok(Tree {
        ecosystem: Ecosystem::Npm,
        source: path.to_path_buf(),
        packages,
        edges,
        roots,
    })
}

/// Where the tarball came from, as far as the file says.
///
/// pnpm records the *kind* of resolution and not the URL: a registry install
/// is `{integrity: …}` and nothing else, while a direct download carries
/// `tarball`, a git checkout carries `repo`/`commit`, and a local package
/// carries `directory`. All 850 entries in the fixture are the first shape.
///
/// The npm reader can do better than this, because `resolved` holds the actual
/// URL and it can check for `registry.npmjs.org`. pnpm keeps the registry in
/// `.npmrc`, not in the lockfile, so a package pulled from a *private*
/// registry is indistinguishable here from a public one and reads as
/// `Registry`. That is a real gap and the honest place to say so: the name
/// rules will ask a public corpus about a name it was never going to contain.
/// Marking everything `Elsewhere` instead would trade that for switching the
/// rules off on every pnpm project, which is worse.
fn origin_of(entry: &Value) -> Origin {
    let Some(res) = entry.get("resolution").and_then(Value::as_mapping) else {
        return Origin::Elsewhere;
    };
    let off_registry = ["tarball", "repo", "commit", "directory", "type"]
        .iter()
        .any(|k| res.contains_key(*k));
    if res.contains_key("integrity") && !off_registry {
        Origin::Registry
    } else {
        Origin::Elsewhere
    }
}

/// `name@version` split at the last `@`, so `@babel/core@7.27.1` keeps its
/// scope. A key with no version at all keeps the whole string as the name.
fn split_key(key: &str) -> (&str, &str) {
    match key.rfind('@') {
        Some(i) if i > 0 => (&key[..i], &key[i + 1..]),
        _ => (key, ""),
    }
}

/// Drop the peer suffix. `astro@5.7.10(jiti@2.4.2)` is installed from the
/// `astro@5.7.10` tarball, and the `packages` section only knows the latter.
fn base_of(key: &str) -> &str {
    match key.find('(') {
        Some(i) => &key[..i],
        None => key,
    }
}

/// A snapshot's resolved edges: `name` to the version string that identifies
/// the snapshot it points at.
///
/// The npm reader counts `peerDependencies` as evidence too. Here it would be
/// double counting: pnpm resolves a satisfied peer *into* the snapshot's
/// `dependencies` — `@astrojs/check@0.9.4` declares a peer on typescript and
/// its snapshot lists `typescript: 5.8.3` outright — so the edge is already in
/// this iterator. The unresolved ones are the `transitivePeerDependencies`
/// list, which is deliberately absent: it is a record of names pnpm could
/// *not* resolve, and half of them are not in the tree at all.
fn resolved_deps(snap: &Value) -> impl Iterator<Item = (&str, &str)> {
    ["dependencies", "optionalDependencies"]
        .into_iter()
        .filter_map(move |field| snap.get(field).and_then(Value::as_mapping))
        .flat_map(|map| {
            map.iter()
                .filter_map(|(name, v)| Some((name.as_str(), v.as_str()?)))
        })
}

/// An importer's dependencies, taking the resolved `version` and ignoring the
/// `specifier` beside it. The specifier is the range in package.json; the
/// version is what got installed, and it is the one that matches a key.
fn declared_deps(importer: &Value) -> impl Iterator<Item = (&str, &str)> {
    ["dependencies", "devDependencies", "optionalDependencies"]
        .into_iter()
        .filter_map(move |field| importer.get(field).and_then(Value::as_mapping))
        .flat_map(|map| {
            map.iter()
                .filter_map(|(name, entry)| Some((name.as_str(), entry.get("version")?.as_str()?)))
        })
}
