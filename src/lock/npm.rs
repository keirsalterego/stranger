//! package-lock.json, lockfileVersion 2 and 3.
//!
//! The awkward part of this format is that `packages` is keyed by install
//! *path*, not by name — `node_modules/@babel/core`, and for a duplicated
//! version `node_modules/eslint/node_modules/semver`. So resolving one
//! package's dependency to another package's entry means reproducing npm's
//! own lookup: try the nearest `node_modules` directory, then walk up.
//!
//! Getting that wrong does not produce a parse error. It produces a graph with
//! the wrong edges, which silently corrupts in-degree, which is the clause the
//! slopsquat rule leans on hardest. 184 of the 1,390 entries in the npm-xl
//! fixture are nested, so this is not a rare path.

use crate::error::{Error, Result};
use crate::json::{self, Value};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use std::collections::HashMap;
use std::path::Path;

const NM: &str = "node_modules/";

pub fn read(path: &Path, src: &str) -> Result<Tree> {
    let doc = json::parse(src)?;

    // Version 1 kept the tree in a nested `dependencies` object and had no
    // `packages` map at all. Refusing it by name beats mis-reading it: the
    // reader would find no packages and cheerfully report a clean project.
    match doc.get("lockfileVersion").and_then(|v| match v {
        Value::Number(n) => Some(*n),
        _ => None,
    }) {
        Some(n) if n >= 2.0 => {}
        Some(n) => {
            return Err(Error::usage(format!(
                "{}: lockfileVersion {n} is not supported; stranger reads 2 and 3. \
                 Run `npm install` with npm 7 or newer to upgrade the file.",
                path.display()
            )));
        }
        None => {
            return Err(Error::usage(format!(
                "{}: no lockfileVersion field; this does not look like a package-lock.json",
                path.display()
            )));
        }
    }

    let entries = doc
        .get("packages")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::usage(format!("{}: no `packages` map", path.display())))?;

    let mut packages = Vec::with_capacity(entries.len());
    let mut index: HashMap<&str, usize> = HashMap::with_capacity(entries.len());

    for (key, entry) in entries {
        if key.is_empty() {
            continue; // the root project, handled below
        }
        index.insert(key.as_str(), packages.len());
        packages.push(Package {
            name: name_from_key(key).to_string(),
            version: entry
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            key: key.clone(),
            dev: entry.get("dev").and_then(Value::as_bool).unwrap_or(false),
            optional: entry
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            first_party: is_first_party(key, entry),
            install_script: entry
                .get("hasInstallScript")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_integrity: entry.get("integrity").is_some(),
            origin: match entry.get("resolved").and_then(Value::as_str) {
                Some(url) if url.starts_with("https://registry.npmjs.org/") => Origin::Registry,
                // A private registry, a tarball URL, a github: spec, or no
                // `resolved` at all (workspace links). The npm corpus is a
                // sample of the public registry and knows nothing about these.
                _ => Origin::Elsewhere,
            },
            // The `^`s and `~`s live in package.json. A package-lock entry is
            // the resolver's answer, and the answer is one version.
            pinned: Pin::Exact,
        });
    }

    // A dependency edge is only evidence that a package is real if a
    // *stranger* wrote it. Edges out of the root manifest are not evidence —
    // that manifest is the thing under audit. Neither are edges out of a
    // workspace member, and that is not a detail: both monorepo fixtures here
    // declare `workspaces` and keep almost nothing in the root, so a
    // hallucinated name added to `apps/desktop/package.json` would otherwise
    // arrive with an in-edge and never be looked at. Same manifest, same
    // author, same lack of evidence. Those land in `roots` instead.
    let mut edges = Vec::new();
    let mut roots = Vec::new();

    let root = entries.get("").unwrap_or(&Value::Null);
    roots.extend(dependency_names(root).filter_map(|dep| resolve(&index, "", dep)));

    for (key, entry) in entries {
        if key.is_empty() {
            continue;
        }
        let from = index[key.as_str()];
        let manifest_we_are_auditing = packages[from].first_party;
        for dep in dependency_names(entry) {
            let Some(to) = resolve(&index, key, dep) else {
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
    // A workspace member is not one of its own direct dependencies.
    roots.retain(|&i| !packages[i].first_party);

    Ok(Tree {
        ecosystem: Ecosystem::Npm,
        source: path.to_path_buf(),
        packages,
        edges,
        roots,
    })
}

/// `node_modules/a/node_modules/@scope/b` is the package `@scope/b`. The last
/// `node_modules/` wins; a scope slash after it is part of the name.
fn name_from_key(key: &str) -> &str {
    match key.rfind(NM) {
        Some(i) => &key[i + NM.len()..],
        None => key,
    }
}

/// A key with no `node_modules/` in it is a workspace directory — somebody in
/// this repo wrote it. So is a `link: true` entry, which is the symlink npm
/// leaves in `node_modules` pointing at one. Seven of the 1,390 entries in the
/// npm-xl fixture are links; leaving them in makes every monorepo scan noise.
fn is_first_party(key: &str, entry: &Value) -> bool {
    !key.contains(NM) || entry.get("link").and_then(Value::as_bool).unwrap_or(false)
}

/// Every field where a maintainer named another package.
///
/// `peerDependencies` is in here on purpose. A peer dep is still a real
/// maintainer writing down a real name, which is exactly the evidence the
/// slopsquat rule is looking for, and counting it can only ever make the rule
/// *more* conservative — an extra in-edge suppresses a finding, never invents
/// one. `devDependencies` only ever appears on the root entry.
fn dependency_names(entry: &Value) -> impl Iterator<Item = &str> {
    [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ]
    .into_iter()
    .filter_map(move |field| entry.get(field).and_then(Value::as_object))
    .flat_map(|map| map.keys().map(String::as_str))
}

/// npm's own resolution order, walking up the install path.
///
/// A dependency `c` of the package at `node_modules/a/node_modules/b` is the
/// first of these that exists:
///
/// ```text
/// node_modules/a/node_modules/b/node_modules/c
/// node_modules/a/node_modules/c
/// node_modules/c
/// ```
fn resolve(index: &HashMap<&str, usize>, from: &str, dep: &str) -> Option<usize> {
    let mut prefix = from;
    loop {
        // ponytail: one allocation per probe. ~14k of them on the largest
        // fixture, which does not show up next to the JSON parse. A reused
        // String buffer is the upgrade if it ever does.
        let candidate = if prefix.is_empty() {
            format!("{NM}{dep}")
        } else {
            format!("{prefix}/{NM}{dep}")
        };
        if let Some(&i) = index.get(candidate.as_str()) {
            return Some(i);
        }
        if prefix.is_empty() {
            return None;
        }
        prefix = match prefix.rfind("/node_modules/") {
            Some(i) => &prefix[..i],
            None => "",
        };
    }
}
