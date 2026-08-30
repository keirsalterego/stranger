//! pnpm-lock.yaml, lockfileVersion 9 and 6.
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
//! # Version 6 is the same file with two sections fused
//!
//! pnpm 8 wrote lockfileVersion 6, which is still on disk in a great many
//! repositories — one of the four pnpm lockfiles on the machine this was
//! written on. It carries the same information in a different arrangement,
//! and the differences are exactly four:
//!
//! - There is no `snapshots` section. A `packages` entry holds its own
//!   resolved `dependencies`, so the tarball list and the instance list are
//!   one section. Fusing them back is what `snapshot_source` does.
//! - Keys start with `/`: `/@eslint/js@9.19.0`, and the peer suffix rides on
//!   the `packages` key rather than on a separate snapshot key. Two entries
//!   for one tarball at different peers therefore collapse to one `Package`,
//!   which is what v9 already reports and what keeps a count comparable
//!   across the two.
//! - A single-project repo writes `dependencies` and `devDependencies` at
//!   the top level and no `importers` at all. The document *is* the importer,
//!   so `declared_deps` is handed the document.
//! - It records two things v9 gave up, and both are read below.
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
//! - **Install scripts, on v9 only.** lockfileVersion 6 has `requiresBuild`
//!   and 9 dropped it. `hasBin: true` appears 42 times in the v9 fixture and
//!   is *not* a substitute — it means the package ships a `bin` entry to
//!   symlink, which is not code running at install time. Mapping one to the
//!   other would put 42 High findings in the report, all of them invented.
//!   So `install_script` is false for every package in a v9 file and
//!   `rules::scripts` correctly says nothing about that tree, while a v6 file
//!   gets the flag read straight off the entry. This is the one axis on which
//!   the older format is worth more, and it is worth saying that the newer
//!   file is the one that can hide a build step.
//! - **Dev-only packages, on v9 only.** pnpm 9 records dev-ness on the
//!   importer's manifest, not on the package, and does not mark the
//!   transitive closure, so `dev` is false throughout rather than guessed at
//!   with a graph walk. v6 marks it per entry — all 90 of them in the v6
//!   fixture — and that is read.
//! - **First-party packages, on v9 only.** A v9 workspace member lives in
//!   `importers` and never in `packages`, and pnpm writes its dependents'
//!   references to it as `link:../name`, which resolves to no package entry.
//!   So `Package::first_party` is false throughout a v9 file for want of
//!   anything to be true about.
//!
//!   v6 does write them down. A `file:` dependency gets a real `packages`
//!   entry keyed by the path — `file:buildscripts/eslint-plugin-mongodb` —
//!   with `resolution: {directory: …, type: directory}` and its actual name in
//!   a `name` field, because the key has no version to carry it. Reading that
//!   key as a package name audits the project's own code as a stranger, and
//!   `stranger tree eslint-plugin-mongodb` could not find it under the name
//!   its own repository uses. Both are fixed by taking the `name` field and
//!   setting `first_party`, which is the same call `npm::is_first_party`
//!   makes about a workspace key.

use crate::error::{Error, Result};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use crate::yaml::{self, Value};
use std::collections::HashMap;
use std::path::Path;

pub fn read(path: &Path, src: &str) -> Result<Tree> {
    let doc = yaml::parse(src)?;

    // pnpm quotes the version, so it arrives as the string "9.0" — and this
    // parser does not turn that into a float, which is the point. Refusing an
    // older layout by name beats mis-reading it: 5 and below key their
    // packages `/name/version`, and a reader that guessed would split every
    // scoped name in the wrong place and report a tree of strangers.
    let major = match doc.get("lockfileVersion").and_then(Value::as_str) {
        Some(v) if v.split('.').next() == Some("9") => 9,
        Some(v) if v.split('.').next() == Some("6") => 6,
        Some(v) => {
            return Err(Error::usage(format!(
                "{}: lockfileVersion {v} is not supported; stranger reads 9 and 6. \
                 Run `pnpm install` with pnpm 8 or newer to upgrade the file.",
                path.display()
            )));
        }
        None => {
            return Err(Error::usage(format!(
                "{}: no lockfileVersion field; this does not look like a pnpm-lock.yaml",
                path.display()
            )));
        }
    };

    // A project with no third-party dependencies writes a valid v9 lockfile
    // with no `packages:` section in it at all, and the honest reading of that
    // file is a tree with nothing in it. Refusing it made "you depend on
    // nobody" indistinguishable from "your lockfile is broken", which is the
    // wrong answer to give the one project that has nothing to audit.
    let entries = doc.get("packages").and_then(Value::as_mapping);
    let count = entries.map_or(0, |e| e.len());

    let mut packages = Vec::with_capacity(count);
    let mut index: HashMap<&str, usize> = HashMap::with_capacity(count);

    for (key, entry) in entries.into_iter().flatten() {
        // v9 keys are already bare; on v6 this drops the `/` and the peer
        // suffix, which is what collapses two instances of one tarball into
        // the single entry v9 would have written.
        let base = base_of(key);
        if index.contains_key(base) {
            continue;
        }
        let (mut name, version) = split_key(base);
        // A v6 `file:` key is a path and carries no version, so the name it
        // splits to is the whole path. The entry writes the real one down.
        let first_party = is_directory(entry);
        if let Some(declared) = entry.get("name").and_then(Value::as_str) {
            name = declared;
        }
        index.insert(base, packages.len());
        packages.push(Package {
            name: name.to_string(),
            version: version.to_string(),
            key: base.to_string(),
            // Both fields are v6-only and absent from every v9 file, so
            // `false` here is the format saying nothing rather than this
            // reader deciding something. See the module header.
            dev: entry.get("dev").and_then(Value::as_bool) == Some(true),
            optional: false,
            first_party,
            install_script: entry.get("requiresBuild").and_then(Value::as_bool) == Some(true),
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
    // v6 has no `snapshots`: a `packages` entry carries its own resolved
    // edges, so it plays both parts. Everything downstream is identical,
    // because `base_of` already made the two key shapes one shape.
    let snapshots = match major {
        6 => entries,
        _ => doc.get("snapshots").and_then(Value::as_mapping),
    };
    let mut edges = Vec::new();

    for (key, snap) in snapshots.into_iter().flatten() {
        let Some(&from) = index.get(base_of(key)) else {
            continue;
        };
        if snap.get("optional").and_then(Value::as_bool) == Some(true) {
            packages[from].optional = true;
        }
        for (name, version) in resolved_deps(snap) {
            // One allocation per edge, 1,851 of them on this file.
            // Invisible next to the parse. A borrowed two-part key is the
            // upgrade if it ever shows up in a profile.
            let want = key_for(name, version);
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
    //
    // A v6 workspace still writes `importers`. A v6 single project writes its
    // manifest at the top level instead, and then the document itself is the
    // one importer — same three fields, same `{specifier, version}` shape, so
    // `declared_deps` takes it unchanged.
    let mut roots = Vec::new();
    let importers = doc.get("importers").and_then(Value::as_mapping);
    let manifests: Vec<&Value> = match importers {
        Some(map) => map.values().collect(),
        None if major == 6 => vec![&doc],
        None => Vec::new(),
    };
    for importer in manifests {
        for (name, version) in declared_deps(importer) {
            let want = key_for(name, version);
            if let Some(&to) = index.get(base_of(&want)) {
                roots.push(to);
            }
        }
    }

    // The manifest under audit is not one of its own dependencies. v9 gets
    // this for free because a `link:` resolves to nothing; v6 has to be told,
    // now that a `file:` dependency resolves to a real entry.
    roots.retain(|&i| !packages[i].first_party);
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
        records_edges: true,
        // The whole reason this is a field rather than a `match` on the
        // filename: one name, two answers.
        records_install_scripts: major == 6,
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

/// The `packages` key a dependency's recorded version points at.
///
/// Normally `name@version`. A `file:` or `link:` version is a path, and pnpm
/// keys the entry by that path alone with no name in front of it, so joining
/// the two produces a key that is in no lockfile. Both spellings appear at v6;
/// v9 writes `link:` and gives it no entry to find.
fn key_for(name: &str, version: &str) -> String {
    if version.starts_with("file:") || version.starts_with("link:") {
        version.to_string()
    } else {
        format!("{name}@{version}")
    }
}

/// A package that is a directory on this disk rather than a tarball from a
/// registry: somebody in this repo wrote it, so no rule should call it a
/// stranger.
fn is_directory(entry: &Value) -> bool {
    entry
        .get("resolution")
        .and_then(|r| r.get("type"))
        .and_then(Value::as_str)
        == Some("directory")
}

/// Drop the v6 leading slash and the peer suffix, leaving the bare
/// `name@version` both versions agree on. `/astro@5.7.10(jiti@2.4.2)` and
/// `astro@5.7.10(jiti@2.4.2)` are installed from the `astro@5.7.10` tarball,
/// and that is the only key the index holds.
fn base_of(key: &str) -> &str {
    let key = key.strip_prefix('/').unwrap_or(key);
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
