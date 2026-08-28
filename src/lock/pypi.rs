//! poetry.lock and uv.lock — the two Python lockfiles that record a graph.
//!
//! One module, two readers. `pip.rs` stays next door because
//! `requirements.txt` is a resolver's *input* and these are its *output*;
//! they share nothing but the ecosystem tag. These two share the one piece
//! worth writing exactly once — turning a dependency's name into an entry —
//! and that piece is where this whole file can go quietly wrong, so it exists
//! in a single place rather than in two files that drift.
//!
//! # Why bother, when `pip.rs` already reads Python
//!
//! `requirements.txt` records no edges, so every package in it has in-degree
//! 0 and the slopsquat rule's third clause — "nothing depends on this name" —
//! eliminates nothing. README LIMITS shows that costing a real false positive
//! on `tensorflow-gpu`. Both formats here record the resolved graph, which is
//! the entire reason to read them: a reader that produced packages and no
//! edges would leave the rule exactly as weak as it already was.
//!
//! # What poetry.lock records
//!
//! Every entry is `[[package]]` with `name`, `version`, `optional`, poetry
//! 2.x's `groups`, a `files` array of `{file, hash}`, and a repeated
//! `[package.dependencies]` sub-table whose *keys* are the dependency names —
//! 112 of those in poetry-m, 20 in poetry-s.
//!
//! `[package.extras]` is not read, and uv's `[package.optional-dependencies]`
//! is, which looks inconsistent and is not. uv's optional block holds
//! *resolved* references: `davey` is in uv-m because somebody asked for
//! `discord-py[voice]`, so that is a real install edge. poetry's extras block
//! is the package's metadata copied verbatim whether anyone asked or not —
//! 1,049 PEP 508 strings in poetry-m, of which 758 name packages that are not
//! in the lock at all. Feeding the other 291 into `edges` would put
//! non-install edges in the graph and collapse the in-degree derivation that
//! `roots` depends on two paragraphs below: a package mentioned by somebody's
//! unrequested `docs` extra would stop looking like a root, when it is one.
//!
//! ponytail: that costs the slopsquat rule 291 pieces of real "a maintainer
//! has heard of this name" evidence in poetry-m, and losing evidence makes
//! the rule fire *more*. The upgrade is a `mentioned` set on `Tree`, separate
//! from `edges`, for clauses that want weaker evidence than an install edge —
//! not a wider definition of `edges`.
//!
//! It does not record the root project. There is no `[[package]]` for the
//! thing being locked, and its direct dependencies live in `pyproject.toml`,
//! which is not this file. See `roots` below.
//!
//! # What uv.lock records
//!
//! `[[package]]` with `name`, `version`, `source`, `sdist`/`wheels` carrying
//! hashes, and dependencies as arrays of inline tables — `dependencies = [
//! { name = "pydantic" } ]` — plus `[package.optional-dependencies]` keyed by
//! extra. uv-m has 141 `dependencies` arrays and 7 optional blocks, and no
//! `[[package.dependencies]]` array-of-tables anywhere; that shape does not
//! occur.
//!
//! It *does* record the root: `source = { editable = "." }` marks the project
//! itself (uv-m has one, `hermes-agent`), and its dependency list is the
//! answer poetry makes you infer.
//!
//! # What neither records
//!
//! **Install-time code.** No field in either format says a package runs
//! `setup.py` at install time, so `install_script` is `false` on every entry
//! here and the scripts rule never fires on a Python tree. There is no honest
//! proxy for it — an sdist with no wheel is *suggestive* and is not the same
//! claim — so nothing is invented. This is a real blind spot relative to npm.
//!
//! **A dev split, in uv's case.** uv-m has no `dev-dependencies` at all, and
//! group membership in uv attaches to the edge rather than the package, so
//! `dev` is `false` on every uv entry. poetry does record it, per package, in
//! `groups`.

use crate::corpus;
use crate::error::{Error, Result};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use crate::toml::{self, Value};
use std::collections::HashMap;
use std::path::Path;

pub fn poetry(path: &Path, src: &str) -> Result<Tree> {
    let doc = toml::parse(src)?;
    let entries = entries(&doc, path, "metadata", "lock-version")?;

    let mut packages = Vec::with_capacity(entries.len());
    for (n, entry) in entries.iter().enumerate() {
        let name = name_of(entry, path, n)?;
        packages.push(Package {
            name: name.to_string(),
            version: string(entry, "version").to_string(),
            key: name.to_string(),
            dev: is_dev(entry),
            optional: entry
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            first_party: entry
                .get("source")
                .and_then(|s| s.get("type"))
                .and_then(Value::as_str)
                == Some("directory"),
            install_script: false,
            // poetry writes `[package.source]` only when the package did *not*
            // come from the default index. Anything with one — git, url, file,
            // directory, or a `legacy` custom index — is outside the PyPI the
            // corpus samples, so the name rules have nothing to go on.
            origin: match entry.get("source") {
                None => Origin::Registry,
                Some(_) => Origin::Elsewhere,
            },
            // `files = []` is what poetry writes for a git or path source, so
            // the empty case is a genuine "no hash recorded" rather than a
            // missing field. Two entries across the two fixtures.
            has_integrity: entry
                .get("files")
                .and_then(Value::as_array)
                .is_some_and(|files| files.iter().any(|f| f.get("hash").is_some())),
            pinned: Pin::Exact,
        });
    }

    let index = Index::build(&packages);
    let mut graph = Graph::default();

    for (from, entry) in entries.iter().enumerate() {
        let Some(deps) = entry.get("dependencies").and_then(Value::as_table) else {
            continue;
        };
        let ours = packages[from].first_party;
        for dep in deps.keys() {
            for &to in index.lookup(dep) {
                graph.link(ours, from, to);
            }
        }
    }

    // poetry.lock does not say which packages the project asked for directly.
    // What it does guarantee is that every entry got in because something
    // wanted it, so an entry nothing else depends on can only have arrived
    // through the root's own manifest. That is a derivation, not a record,
    // and it is wrong in one direction: a direct dependency that some other
    // package also needs has in-degree > 0 and will not appear here, so
    // `Tree::direct()` under-reports on poetry. The alternative is reading
    // `pyproject.toml`, which means resolving a sibling file and trusting it
    // to still match the lock.
    //
    // `[extras]` at the bottom of the file does list some of the root's
    // names, but only the optional ones, and every name in it is already
    // caught by the in-degree test — poetry-s's `gui = ["PyGObject"]` points
    // at `pygobject`, which nothing depends on.
    graph.derive_roots(&packages);
    Ok(graph.finish(Ecosystem::PyPi, path, packages))
}

pub fn uv(path: &Path, src: &str) -> Result<Tree> {
    let doc = toml::parse(src)?;
    let entries = entries(&doc, path, "requires-python", "")?;

    let mut packages = Vec::with_capacity(entries.len());
    for (n, entry) in entries.iter().enumerate() {
        let name = name_of(entry, path, n)?;
        packages.push(Package {
            name: name.to_string(),
            version: string(entry, "version").to_string(),
            key: name.to_string(),
            dev: false,
            optional: false,
            first_party: is_local(entry),
            install_script: false,
            // uv names the source kind explicitly. `registry` is PyPI (or a
            // configured index — uv does not distinguish in the lock, and
            // pretending otherwise would be a guess); everything else is git,
            // a URL, or the project's own directories.
            origin: match entry.get("source").and_then(Value::as_table) {
                Some(src) if src.contains_key("registry") => Origin::Registry,
                _ => Origin::Elsewhere,
            },
            has_integrity: entry.get("sdist").is_some_and(|s| s.get("hash").is_some())
                || entry
                    .get("wheels")
                    .and_then(Value::as_array)
                    .is_some_and(|w| w.iter().any(|x| x.get("hash").is_some())),
            pinned: Pin::Exact,
        });
    }

    let index = Index::build(&packages);
    let mut graph = Graph::default();

    for (from, entry) in entries.iter().enumerate() {
        let ours = packages[from].first_party;
        for dep in dependencies(entry) {
            let Some(name) = dep.get("name").and_then(Value::as_str) else {
                continue;
            };
            let candidates = index.lookup(name);
            // uv forks one name into several entries when the resolution
            // splits on a marker — `scipy` 1.17.1 and 1.18.0 are the only
            // such pair in uv-m — and the dependency then carries `version`
            // to say which fork it meant. Honouring that keeps the other fork
            // from looking like a package nothing depends on.
            //
            // With no version, or one that matches nothing, every candidate
            // gets the edge. Over-linking can only add an in-edge, and an
            // extra in-edge suppresses a slopsquat finding — it never invents
            // one — so the loose case fails in the safe direction.
            let exact = dep
                .get("version")
                .and_then(Value::as_str)
                .and_then(|v| candidates.iter().find(|&&i| packages[i].version == v));
            for &to in exact.map_or(candidates, std::slice::from_ref) {
                graph.link(ours, from, to);
            }
        }
    }

    // No derivation needed: the editable entry above *is* the root manifest,
    // and its dependency list went into `roots` rather than `edges` for the
    // same reason npm's workspace members do — the manifest under audit is
    // not evidence that anyone else has heard of a name.
    Ok(graph.finish(Ecosystem::PyPi, path, packages))
}

/// Normalised name to every entry carrying it.
///
/// **Both sides normalise, and that is the reason this type exists.** PEP 503
/// makes `Foo.Bar`, `foo-bar` and `foo_bar` one project on PyPI, and neither
/// format spells names consistently: poetry writes package names normalised
/// but dependency keys exactly as the depending project typed them. In
/// poetry-s that is `charset_normalizer` pointing at a package called
/// `charset-normalizer`, `"jaraco.classes"` at `jaraco-classes`, and
/// `SecretStorage` at `secretstorage`; poetry-m adds `PyYAML`, `Click` and
/// `typing_extensions`.
///
/// Comparing the raw strings drops those edges. Nothing errors — the lookup
/// just misses — and the packages on the far end acquire in-degree 0, which
/// is precisely the shape the slopsquat rule fires on. A missing edge here
/// does not fail loudly; it manufactures a finding.
///
/// `Package::name` still holds the string as written, because the report has
/// to quote something the reader can find in the file. Normalisation happens
/// here and in `corpus`, at comparison time, and nowhere else.
struct Index(HashMap<String, Vec<usize>>);

impl Index {
    fn build(packages: &[Package]) -> Index {
        let mut map: HashMap<String, Vec<usize>> = HashMap::with_capacity(packages.len());
        for (i, p) in packages.iter().enumerate() {
            map.entry(corpus::normalize(Ecosystem::PyPi, &p.name))
                .or_default()
                .push(i);
        }
        Index(map)
    }

    fn lookup(&self, name: &str) -> &[usize] {
        self.0
            .get(&corpus::normalize(Ecosystem::PyPi, name))
            .map_or(&[], Vec::as_slice)
    }
}

#[derive(Default)]
struct Graph {
    edges: Vec<(usize, usize)>,
    roots: Vec<usize>,
}

impl Graph {
    /// `ours` is npm.rs's rule in one word: an edge out of a package somebody
    /// in this repo wrote is that person's own manifest, not a stranger
    /// vouching for a name. Those land in `roots`.
    fn link(&mut self, ours: bool, from: usize, to: usize) {
        // A package is not evidence for itself. poetry's `[package.extras]`
        // routinely names the package it belongs to (`cachecontrol` lists
        // `CacheControl[filecache,redis]`), and while extras are not read
        // here, a self-edge from any source would hand a name an in-degree
        // that suppresses a finding about it.
        if to == from {
            return;
        }
        if ours {
            self.roots.push(to);
        } else {
            self.edges.push((from, to));
        }
    }

    fn derive_roots(&mut self, packages: &[Package]) {
        let mut degree = vec![0u32; packages.len()];
        for &(_, to) in &self.edges {
            degree[to] += 1;
        }
        self.roots
            .extend((0..packages.len()).filter(|&i| degree[i] == 0));
    }

    fn finish(mut self, ecosystem: Ecosystem, path: &Path, packages: Vec<Package>) -> Tree {
        self.roots.sort_unstable();
        self.roots.dedup();
        // Somebody in this repo wrote it; it is not one of its own direct
        // dependencies, and it is not a stranger.
        self.roots.retain(|&i| !packages[i].first_party);
        Tree {
            ecosystem,
            source: path.to_path_buf(),
            packages,
            edges: self.edges,
            roots: self.roots,
        }
    }
}

/// The `[[package]]` array, once the file has proved it is the format its
/// name claims.
///
/// The marker check is not ceremony. Dispatch is by filename, and a
/// `Cargo.lock` copied to `uv.lock` parses as TOML, has a top-level `version`
/// and 700 `[[package]]` entries, and would read as a clean 700-package
/// Python project — cargo writes its dependencies as strings rather than
/// inline tables, so every edge would silently vanish. `requires-python` and
/// `[metadata] lock-version` appear in every file their own tool writes and
/// in nothing else here.
///
/// An absent `package` array is *not* an error: a project with no
/// dependencies gets a lockfile with a header and nothing under it, and
/// refusing that is a false alarm about a file that is simply empty.
fn entries<'a>(doc: &'a Value, path: &Path, marker: &str, sub: &str) -> Result<&'a [Value]> {
    let found = match doc.get(marker) {
        Some(v) if sub.is_empty() => Some(v),
        Some(v) => v.get(sub),
        None => None,
    };
    if found.is_none() {
        let key = if sub.is_empty() {
            marker.to_string()
        } else {
            format!("{marker}.{sub}")
        };
        return Err(Error::usage(format!(
            "{}: no `{key}`; this is not the lockfile its name claims to be",
            path.display()
        )));
    }
    Ok(doc
        .get("package")
        .and_then(Value::as_array)
        .unwrap_or(&[][..]))
}

fn name_of<'a>(entry: &'a Value, path: &Path, n: usize) -> Result<&'a str> {
    entry.get("name").and_then(Value::as_str).ok_or_else(|| {
        // No line number: positions die at the end of `toml::parse`. The
        // ordinal is what is left, and it is enough to find the entry.
        Error::usage(format!(
            "{}: `[[package]]` #{} has no `name`",
            path.display(),
            n + 1
        ))
    })
}

fn string<'a>(entry: &'a Value, key: &str) -> &'a str {
    entry.get(key).and_then(Value::as_str).unwrap_or("")
}

/// poetry 2.x's `groups`, falling back to poetry 1.x's `category`.
///
/// The fallback is not decoration. `groups` absent means an older file, and
/// `!groups.contains("main")` on an empty list is `true`, so reading only
/// `groups` would mark every package in a poetry 1.x lock as a dev
/// dependency — a wrong answer delivered confidently. Both fixtures are
/// lock-version 2.1 and take the first arm.
fn is_dev(entry: &Value) -> bool {
    match entry.get("groups").and_then(Value::as_array) {
        Some(groups) => !groups.iter().any(|g| g.as_str() == Some("main")),
        None => entry.get("category").and_then(Value::as_str) == Some("dev"),
    }
}

/// uv names the project's own packages by source kind: `editable` for a
/// workspace member installed in place, `virtual` for one that exists only to
/// hold dependencies, `directory` for a plain path. uv-m has exactly one,
/// `hermes-agent` at `source = { editable = "." }`.
///
/// poetry's equivalent is `[package.source] type = "directory"`, handled
/// inline in `poetry` above. Neither poetry fixture contains one — the only
/// sources there are two `git` and one `legacy` index — so that arm is
/// reasoned from the format rather than measured against a file.
fn is_local(entry: &Value) -> bool {
    entry
        .get("source")
        .and_then(Value::as_table)
        .is_some_and(|s| {
            s.contains_key("editable") || s.contains_key("virtual") || s.contains_key("directory")
        })
}

/// Every inline table in which this uv entry names another package.
///
/// `optional-dependencies` and `dev-dependencies` are tables of group name to
/// array; `dependencies` is the array directly. All three count as evidence
/// for the same reason npm.rs counts `peerDependencies`: a maintainer wrote
/// the name down, and an extra in-edge can only make the slopsquat rule more
/// conservative.
///
/// `[package.metadata] requires-dist` is deliberately skipped. It is the
/// root's `pyproject.toml` copied into the lock — the declaration, not the
/// resolution — and the same entry's `dependencies` already carries the
/// resolved form of it.
fn dependencies(entry: &Value) -> impl Iterator<Item = &Value> {
    let direct = entry
        .get("dependencies")
        .and_then(Value::as_array)
        .unwrap_or(&[][..])
        .iter();
    let grouped = ["optional-dependencies", "dev-dependencies"]
        .into_iter()
        .filter_map(move |field| entry.get(field).and_then(Value::as_table))
        .flat_map(|table| table.values())
        .filter_map(Value::as_array)
        .flatten();
    direct.chain(grouped)
}
