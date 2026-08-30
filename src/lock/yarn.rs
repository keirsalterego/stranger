//! yarn.lock, the v1 format.
//!
//! Not YAML, despite reading like it. yarn v1 wrote its own thing: a flat list
//! of entries at column 0, two-space fields under each, and quoting that is
//! optional except when the value contains a character that would break the
//! line. `yaml.rs` is next door and is deliberately not used here — feeding
//! this file to a YAML parser is how a reader ends up with
//! `lodash@^4.17.20, lodash@~4.17.0` as a single scalar key it then has to
//! take apart anyway.
//!
//! ```text
//! "@babel/code-frame@^7.0.0":
//!   version "7.0.0"
//!   resolved "https://registry.yarnpkg.com/@babel/code-frame/-/..."
//!   integrity sha512-OfC2uemaknXr87bdLUkWog7nYuliM9Ij5HUcajsVcMCpQrcL...
//!   dependencies:
//!     "@babel/highlight" "^7.0.0"
//! ```
//!
//! # The edges are specifiers, not versions
//!
//! This is the one thing that makes yarn different from every other reader
//! here, and getting it wrong produces a tree with no edges rather than an
//! error. A `dependencies` line names a **range**, not a resolved version:
//! `"@babel/highlight" "^7.0.0"`. The entry it points at is the one whose key
//! list contains the literal specifier `@babel/highlight@^7.0.0`, and only the
//! key list can answer that — `^7.0.0` never appears in the target entry's
//! `version` field, which says `7.0.0`.
//!
//! So the index is keyed by specifier and not by `name@version`, and one entry
//! is reachable through several keys:
//!
//! ```text
//! lodash@^4.17.20, lodash@~4.17.0:
//!   version "4.17.21"
//! ```
//!
//! Both specifiers resolve to that single package. Resolving edges by
//! `name@version` instead would match nothing at all, because no dependency
//! line anywhere in the file is written that way.
//!
//! # What the format does not record
//!
//! **The root manifest.** yarn keeps direct dependencies in `package.json`,
//! which is not this file, so `roots` is derived the way `pypi.rs` derives it:
//! an entry nothing else depends on can only have arrived through the root's
//! manifest. That derivation is wrong in one direction — a direct dependency
//! that something else also needs has an in-edge and drops out of the count —
//! and the same caveat is on the poetry reader for the same reason.
//!
//! **Install scripts, dev-ness, and workspace membership.** None of the three
//! is in a v1 lockfile. `install_script`, `dev` and `first_party` are false
//! throughout and `records_install_scripts` is false, so the scripts rule says
//! *no signal in this format* rather than *nothing found*.

use crate::error::{Error, Result};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use std::collections::HashMap;
use std::path::Path;

pub fn read(path: &Path, src: &str) -> Result<Tree> {
    // Berry keys its packages `name@npm:range` and carries a `__metadata`
    // block with its own version counter. It is a real YAML document and a
    // different format wearing the same filename; refusing it by name beats
    // reading half of it. The v1 header is a comment, so its absence is not
    // proof of anything on its own.
    if src
        .lines()
        .any(|l| l.trim_start().starts_with("__metadata:"))
    {
        return Err(Error::usage(format!(
            "{}: this is a Yarn Berry (v2+) lockfile; stranger reads the v1 format",
            path.display()
        )));
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut lines = src.lines().enumerate().peekable();

    while let Some((i, raw)) = lines.next() {
        let line = i as u32 + 1;
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        // An entry header is the only thing at column 0.
        if raw.starts_with([' ', '\t']) {
            return Err(syntax("indented line outside any entry", line, 1));
        }
        let header = raw.strip_suffix(':').ok_or_else(|| {
            syntax(
                "entry header does not end in `:`",
                line,
                raw.chars().count() as u32 + 1,
            )
        })?;

        let mut entry = Entry {
            specifiers: specifiers(header, line)?,
            line,
            ..Entry::default()
        };

        // Fields belong to this entry until the next column-0 line.
        while let Some(&(j, field)) = lines.peek() {
            if !field.starts_with([' ', '\t']) && !field.trim().is_empty() {
                break;
            }
            lines.next();
            let fline = j as u32 + 1;
            let trimmed = field.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // A bare `name:` with nothing after it opens a nested block.
            // Three of them hold `name range` pairs and are edges; the rest —
            // `engines`, `os`, `cpu`, and the two `…Meta` blocks — hold things
            // this tool does not report, and their contents fall through to
            // the `key value` arm below and are dropped there by name.
            //
            // `peerDependencies` counts as an edge for the reason `npm.rs`
            // counts it: a peer dep is a real maintainer writing down a real
            // name, which is the evidence the detection rule wants, and an
            // in-edge can only make that rule quieter. Both readers are
            // `Ecosystem::Npm` and `stranger diff` will put one against the
            // other, so a name with in-degree 1 under npm must not have
            // in-degree 0 under yarn.
            //
            // What none of these headers may do is reach `pair`, which wants a
            // value and errors when a header has none. That is what refused
            // every real lockfile carrying a peer dependency: a syntax error,
            // reported against a file that has none.
            if let Some(block) = trimmed
                .strip_suffix(':')
                .filter(|b| !b.contains(char::is_whitespace))
            {
                entry.in_deps = matches!(
                    block,
                    "dependencies" | "optionalDependencies" | "peerDependencies"
                );
                continue;
            }
            // A dependency line is nested one level deeper than the block
            // header that opened it. Depth is what separates
            // `  version "7.0.0"` from `    lodash "^4.17.0"`, because both
            // are two tokens and neither is quoted in a way the other is not.
            let depth = field.len() - field.trim_start().len();
            if entry.in_deps && depth >= 4 {
                let (name, range) = pair(trimmed, fline)?;
                entry.deps.push(format!("{name}@{range}"));
                continue;
            }
            entry.in_deps = false;
            let (key, value) = pair(trimmed, fline)?;
            match key {
                "version" => entry.version = value.to_string(),
                "resolved" => entry.resolved = value.to_string(),
                "integrity" => entry.integrity = true,
                // `dependencies` blocks aside, a v1 entry also carries
                // `engines`, `os`, `cpu` and `peerDependencies`. None of them
                // change what this tool reports, and refusing an unknown field
                // would break on the next one yarn adds.
                _ => {}
            }
        }

        if entry.version.is_empty() {
            return Err(syntax("entry has no `version` field", entry.line, 1));
        }
        entries.push(entry);
    }

    let mut packages = Vec::with_capacity(entries.len());
    // Specifier -> index. Several specifiers point at one package, which is
    // the whole reason this map exists.
    let mut by_specifier: HashMap<&str, usize> = HashMap::with_capacity(entries.len() * 2);

    for entry in &entries {
        let name = entry
            .specifiers
            .first()
            .map(|(n, _)| *n)
            .ok_or_else(|| syntax("entry header names no package", entry.line, 1))?;
        let index = packages.len();
        for (_, spec) in &entry.specifiers {
            by_specifier.insert(spec, index);
        }
        packages.push(Package {
            name: name.to_string(),
            version: entry.version.clone(),
            key: format!("{name}@{}", entry.version),
            dev: false,
            optional: false,
            first_party: false,
            install_script: false,
            has_integrity: entry.integrity,
            // yarn writes the tarball URL it actually fetched. Both the public
            // registry hosts appear in the wild: `registry.yarnpkg.com` is a
            // CNAME of the npm registry and files written by different yarn
            // versions disagree about which name they record, so a reader that
            // knows only one of them calls half the packages private.
            origin: match entry.resolved.as_str() {
                url if url.starts_with("https://registry.yarnpkg.com/")
                    || url.starts_with("https://registry.npmjs.org/") =>
                {
                    Origin::Registry
                }
                _ => Origin::Elsewhere,
            },
            // The range lives in the key; `version` is what got installed.
            pinned: Pin::Exact,
        });
    }

    let mut edges = Vec::new();
    for (from, entry) in entries.iter().enumerate() {
        for dep in &entry.deps {
            // A dependency yarn did not resolve into this file has no entry to
            // point at. That happens for peer dependencies the user never
            // installed, and it is not an error in the lockfile.
            if let Some(&to) = by_specifier.get(dep.as_str()) {
                edges.push((from, to));
            }
        }
    }

    let mut degree = vec![0u32; packages.len()];
    for &(_, to) in &edges {
        degree[to] += 1;
    }
    let roots: Vec<usize> = (0..packages.len()).filter(|&i| degree[i] == 0).collect();

    edges.sort_unstable();
    edges.dedup();

    Ok(Tree {
        ecosystem: Ecosystem::Npm,
        source: path.to_path_buf(),
        packages,
        edges,
        roots,
        records_edges: true,
        records_install_scripts: false,
    })
}

#[derive(Default)]
struct Entry<'a> {
    /// `(name, full specifier)` for every key this entry answers to.
    specifiers: Vec<(&'a str, &'a str)>,
    version: String,
    resolved: String,
    integrity: bool,
    deps: Vec<String>,
    in_deps: bool,
    line: u32,
}

/// The comma-separated key list on an entry header.
///
/// `lodash@^4.17.20, lodash@~4.17.0` is two specifiers for one package, and
/// `"@babel/core@^7.0.0"` is one that has to lose its quotes before the `@`
/// split, or the scope's leading `@` is no longer at index 0 and the name
/// comes back empty.
fn specifiers(header: &str, line: u32) -> Result<Vec<(&str, &str)>> {
    let mut out = Vec::new();
    for part in header.split(',') {
        let spec = unquote(part.trim());
        if spec.is_empty() {
            continue;
        }
        out.push((split_specifier(spec, line)?, spec));
    }
    if out.is_empty() {
        return Err(syntax("entry header names no package", line, 1));
    }
    Ok(out)
}

/// The package name in `name@range`, split at the last `@` so that a scoped
/// name keeps the one it starts with.
///
/// `@babel/core@^7.0.0` splits at index 11, not 0. A specifier with no `@` at
/// all is a name with no range, which yarn writes for a resolution it pinned
/// by other means.
fn split_specifier(spec: &str, line: u32) -> Result<&str> {
    match spec.rfind('@') {
        Some(0) | None => Ok(spec),
        Some(i) => {
            let name = &spec[..i];
            if name.is_empty() {
                return Err(syntax(format!("specifier `{spec}` has no name"), line, 1));
            }
            Ok(name)
        }
    }
}

/// `key value` or `key "value"`, split at the first run of whitespace.
///
/// Both halves may be quoted and neither reliably is: yarn quotes only what it
/// must, so `version "7.0.0"` and `integrity sha512-Of...` are the same shape
/// with different quoting.
fn pair(line_text: &str, line: u32) -> Result<(&str, &str)> {
    let (key, value) = line_text
        .split_once(char::is_whitespace)
        .ok_or_else(|| syntax(format!("`{line_text}` is not a `key value` pair"), line, 1))?;
    Ok((unquote(key.trim()), unquote(value.trim())))
}

fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
}

fn syntax(what: impl Into<String>, line: u32, col: u32) -> Error {
    Error::Syntax {
        what: what.into(),
        line,
        col,
    }
}
