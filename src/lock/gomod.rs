//! go.mod — what a module asks for, which is not quite what it gets.
//!
//! Like `requirements.txt` this is a manifest rather than a resolver's answer,
//! and like `requirements.txt` people commit it and treat it as a lockfile.
//! Unlike `requirements.txt` they are mostly right to, and the reason is
//! minimal version selection: the version on a `require` line is a *floor*,
//! and the build picks the maximum floor named anywhere in the module graph.
//! Since Go 1.17 a tidy go.mod lists the whole build list — every indirect
//! module, at the version that was selected — so the file and the build agree
//! unless somebody hand-edited one of them. On a `go 1.16` module the same
//! file lists direct requirements only and the rest of the tree is somewhere
//! this reader cannot see; both fixtures here are 1.17 or later.
//!
//! # `// indirect` is the entire graph
//!
//! There are no edges in this format. A go.mod says *that* a module is needed
//! transitively and never *through what*, so `edges` is empty, every package
//! has in-degree 0, and the slopsquat rule's third clause is vacuous here in
//! exactly the way it is on a `requirements.txt`. The one thing the format
//! does give up is the direct/indirect split, which lands in `roots` — 50
//! direct against 124 indirect across `gomod-m`'s 174 requires.
//!
//! It costs nothing today, because there is no Go corpus and the rule does not
//! run at all on this ecosystem. See `corpus::names` and README LIMITS.
//!
//! # go.sum is not read
//!
//! It was the obvious next file and it earns nothing here. Three reasons, in
//! ascending order of how much they settle it. It holds a line for every
//! module version in the *graph*, not the build list — `go mod tidy` keeps
//! hashes for versions that lost the selection — so counting packages from it
//! overstates the tree. `go mod tidy` also guarantees a line for everything
//! that is in the build, so `has_integrity` computed from it would be a
//! constant `true` and a constant is not a signal. And the field it would
//! populate is presence, never correctness, because the standard library has
//! no SHA-256 to check an `h1:` hash with — the same wall the npm reader hits
//! on `integrity`. A second file, opened by a reader that is handed one
//! string, for a column of `true`.
//!
//! # Directives no fixture here exercises
//!
//! `replace`, `exclude`, `toolchain`, `godebug`, `tool` and `ignore` appear in
//! neither fixture. They are parsed and tested against hand-written input in
//! `tests/gomod.rs`, and that is the honest status: handled, unmeasured. The
//! one of those that changes an answer rather than just being consumed is
//! `replace`, and no real file I have carries the local-path form, so it could
//! only ever have been tested by hand.
//!
//! `retract` is exercised, by `gomod-xs`, and it is the reason the directives
//! that carry no module path are *consumed* rather than skipped: a retract
//! block holds bare versions, and the ones in the wild hold `[v1.11.0,
//! v1.11.2]` ranges too. A reader that skipped to the next line it recognised
//! would read those as module paths and invent packages out of a line that
//! names none.

use crate::error::{Error, Result};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use std::collections::HashMap;
use std::path::Path;

/// Every directive the go command writes, including the three added since 1.23
/// — `godebug`, `tool`, `ignore` — and `toolchain` from 1.21 before them.
/// Anything else is a syntax error rather than a line to skip: the go team adds
/// a directive every couple of releases, and a reader that silently ignores
/// what it has not heard of will one day ignore a `require` spelled slightly
/// wrong.
const DIRECTIVES: &[&str] = &[
    "module",
    "go",
    "toolchain",
    "require",
    "exclude",
    "replace",
    "retract",
    "godebug",
    "tool",
    "ignore",
];

struct Require<'a> {
    path: &'a str,
    version: &'a str,
    indirect: bool,
}

/// The parts of a go.mod worth keeping. `replace` is collected rather than
/// applied on sight because it is allowed either side of the `require` it
/// modifies, and real files put it at the bottom.
#[derive(Default)]
struct Doc<'a> {
    module: Option<&'a str>,
    requires: Vec<Require<'a>>,
    replaced: HashMap<&'a str, &'a str>,
}

pub fn read(path: &Path, src: &str) -> Result<Tree> {
    let mut doc = Doc::default();
    // The directive whose parenthesised block we are inside, with the position
    // of its keyword so an unterminated block is reported where it opened
    // instead of at EOF, which is where the file stops being useful.
    let mut open: Option<(&str, u32, u32)> = None;

    for (i, raw) in src.lines().enumerate() {
        let line = i as u32 + 1;
        // go.mod's comment syntax is `//` and only `//`. There is no `/* */`
        // in this grammar — worth knowing before writing the scanner you do
        // not need — and a stray `/*` reaches the unknown-directive arm below.
        let (body, note) = match raw.find("//") {
            Some(at) => (&raw[..at], raw[at + 2..].trim()),
            None => (raw, ""),
        };
        let toks = tokens(body);
        // Blank, or nothing but a comment. Both are legal anywhere, including
        // between the entries of a block, and `retract` blocks are full of
        // them.
        let Some(&(first, col)) = toks.first() else {
            continue;
        };
        // A module path may be quoted, and the go lexer reads Go string
        // literals when it is. Nothing in the wild does this — a path that
        // needs escaping is a path the proxy cannot fetch — so refusing beats
        // shipping an unescaper for a case with no caller.
        if let Some(&(quoted, at)) = toks.iter().find(|(t, _)| t.starts_with(['"', '`'])) {
            return Err(syntax(
                format!("`{quoted}` is a quoted path and stranger does not read those"),
                line,
                at,
            ));
        }

        match open {
            // Inside a block every line is an entry of the directive that
            // opened it, and the one line that is not is the `)`.
            Some((kind, _, _)) => {
                if first == ")" {
                    if let Some(&(_, after)) = toks.get(1) {
                        return Err(syntax(
                            "`)` closes the block and takes nothing after it",
                            line,
                            after,
                        ));
                    }
                    open = None;
                    continue;
                }
                entry(&mut doc, kind, &toks, note, line, col)?;
            }
            None => {
                if !DIRECTIVES.contains(&first) {
                    return Err(syntax(
                        format!("`{first}` is not a go.mod directive"),
                        line,
                        col,
                    ));
                }
                match toks.get(1) {
                    // `require (` and friends. The paren has to be the last
                    // token on the line: go's own parser wants a newline after
                    // it, and accepting `require (github.com/x v1.0)` here
                    // would be inventing a grammar the toolchain will reject.
                    Some(&("(", _)) => {
                        if let Some(&(_, after)) = toks.get(2) {
                            return Err(syntax(
                                format!(
                                    "`{first} (` opens a block; its entries go on the lines below"
                                ),
                                line,
                                after,
                            ));
                        }
                        open = Some((first, line, col));
                    }
                    _ => entry(&mut doc, first, &toks[1..], note, line, col)?,
                }
            }
        }
    }

    if let Some((kind, line, col)) = open {
        return Err(syntax(format!("`{kind} (` is never closed"), line, col));
    }

    // Cargo's reader refuses a TOML file with no `[[package]]`; this is the
    // same guard. Every go.mod has a module directive, so a file without one
    // is something else that happens to be named go.mod.
    if doc.module.is_none() {
        return Err(Error::usage(format!(
            "{}: no `module` directive; this does not look like a go.mod",
            path.display()
        )));
    }

    let packages: Vec<Package> = doc
        .requires
        .iter()
        .map(|r| {
            let replacement = doc.replaced.get(r.path).copied();
            // `replace github.com/us/internal => ./internal` is npm's
            // `link: true` in a different grammar: the code is in the tree
            // being audited, written by whoever is being audited. Go's test
            // for a filesystem path is a leading `./`, `../` or `/`.
            let local = replacement.is_some_and(|to| to.starts_with(['.', '/']));
            Package {
                name: r.path.to_string(),
                // As written, `v` and all. `v1.2.3` is what the file says and
                // what `go list` prints back.
                version: r.version.to_string(),
                // A module path appears at most once in a require list — two
                // major versions are two paths, `github.com/x/y` and
                // `github.com/x/y/v2` — so the path is already the unique key
                // that Cargo has to build out of a name and a version.
                key: r.path.to_string(),
                // No dev/test split: a module needed only by tests is a
                // require like any other, marked indirect if nothing outside
                // the tests imports it. No optionality either; build tags
                // decide what compiles and the module is fetched regardless.
                dev: false,
                optional: false,
                first_party: local,
                // Not a blank like Cargo's — a measurement. The module system
                // has no install-time hook: `go mod download` fetches and
                // unpacks a zip, and nothing in it runs until you build.
                install_script: false,
                // go.mod records no hash. go.sum does, and is not read; see
                // the module doc for why.
                has_integrity: false,
                // MVS makes the version a floor rather than a range, and on a
                // tidy 1.17+ module the floor is the selection. `Pin::Range`
                // here would fire the pinning rule on all 174 entries of
                // `gomod-m` and be wrong 174 times.
                pinned: Pin::Exact,
                // `Registry` means the module proxy, which is a cache and not
                // a curated index — it will serve any importable path. Nothing
                // reads this today because the Go corpus is empty; it is set
                // truthfully so that it is still true if that changes.
                origin: match replacement {
                    Some(_) => Origin::Elsewhere,
                    None => Origin::Registry,
                },
            }
        })
        .collect();

    let roots = doc
        .requires
        .iter()
        .zip(&packages)
        .enumerate()
        .filter(|(_, (r, p))| !r.indirect && !p.first_party)
        .map(|(i, _)| i)
        .collect();

    Ok(Tree {
        ecosystem: Ecosystem::Go,
        source: path.to_path_buf(),
        packages,
        // Not this reader giving up. The edges are not in the file to read,
        // which is what `records_edges` says out loud so clause 3 does not
        // read an in-degree of 0 as a measurement.
        edges: Vec::new(),
        records_edges: false,
        records_install_scripts: false,
        roots,
    })
}

/// One directive, or one line of a directive's block — the two are the same
/// thing with the keyword stripped, which is why block form costs almost
/// nothing here.
fn entry<'a>(
    doc: &mut Doc<'a>,
    kind: &str,
    args: &[(&'a str, u32)],
    note: &str,
    line: u32,
    col: u32,
) -> Result<()> {
    match kind {
        "module" => {
            let &[(path, _)] = args else {
                return Err(syntax("`module` takes one module path", line, col));
            };
            doc.module = Some(path);
        }
        "require" | "exclude" => {
            let &[(path, _), (version, at)] = args else {
                return Err(syntax(
                    format!("`{kind}` takes a module path and a version"),
                    line,
                    col,
                ));
            };
            if !is_version(version) {
                return Err(syntax(
                    format!("`{version}` is not a module version"),
                    line,
                    at,
                ));
            }
            // An `exclude` names a version the build must *not* select. It is
            // the opposite of a dependency, and minting a Package for it would
            // put something in the package count that is not in the build.
            if kind == "require" {
                doc.requires.push(Require {
                    path,
                    version,
                    indirect: is_indirect(note),
                });
            }
        }
        "replace" => {
            // `old => new`, `old v1.2.3 => new v4.5.6`, `old => ../local`. The
            // arrow is the only token at a fixed place, so find it rather than
            // counting: each side is one or two tokens depending on whether a
            // version was given.
            let Some(arrow) = args.iter().position(|&(t, _)| t == "=>") else {
                return Err(syntax("`replace` needs a `=>`", line, col));
            };
            let (left, right) = args.split_at(arrow);
            let (Some(&(old, _)), Some(&(new, _))) = (left.first(), right.get(1)) else {
                return Err(syntax(
                    "`replace` needs a module path on each side of the `=>`",
                    line,
                    col,
                ));
            };
            // Only the left-hand path is kept. When a replace swaps in a
            // different module the code you build is the right-hand one and
            // the report will still say the left, which is a real inaccuracy
            // and needs a field `Package` does not have. The half that is not
            // lost is the interesting one: a replacement pointing at a
            // directory is first-party code, and that is read above.
            doc.replaced.insert(old, new);
        }
        // Read and dropped. `go`, `toolchain` and `godebug` carry settings
        // rather than modules; `tool` carries package paths that are already
        // required by path elsewhere; `retract` carries this module's own
        // withdrawn versions, bare or as a `[low, high]` range, and none of
        // those is a dependency of anything.
        _ => {}
    }
    Ok(())
}

/// Whitespace-separated tokens with the 1-based column each starts at.
///
/// Columns count characters and not bytes, like every other parser here, so
/// the position lines up with what an editor shows.
fn tokens(line: &str) -> Vec<(&str, u32)> {
    let mut out = Vec::new();
    let mut start: Option<(usize, u32)> = None;

    // Byte offset for slicing, character count for the column: a tab is one of
    // each, and go.mod indents its blocks with tabs.
    for (col, (at, c)) in (1u32..).zip(line.char_indices()) {
        if c.is_whitespace() {
            if let Some((from, from_col)) = start.take() {
                out.push((&line[from..at], from_col));
            }
        } else if start.is_none() {
            start = Some((at, col));
        }
    }
    if let Some((from, from_col)) = start {
        out.push((&line[from..], from_col));
    }
    out
}

/// `go mod tidy` writes exactly `// indirect`, and when the line already
/// carried a comment it writes `// indirect; the old one`. Both mean the same
/// thing, and the second spelling is why this is not an equality check.
fn is_indirect(note: &str) -> bool {
    note == "indirect" || note.starts_with("indirect;")
}

/// A module version: `v1.2.3`, `v2.0.3+incompatible`, or a pseudo-version.
///
/// The core is always three components. go canonicalises `v1.2` to `v1.2.0`
/// before it writes the file, so a two-component version on a require line is
/// a hand-edit rather than something the toolchain produced, and refusing it
/// is right.
///
/// The rest is deliberately loose, and pseudo-versions are why. They are
/// ordinary semver prereleases carrying a UTC timestamp and a 12-hex-digit
/// commit prefix, in three shapes depending on what the commit was tagged
/// after:
///
/// ```text
/// v0.0.0-20240520201108-78e41c74b4b1                 no earlier tag
/// v1.1.2-0.20180830191138-d8f796af33cc               a commit after a release
/// v3.0.1-0.20171022003610-9aa49832a739+incompatible  and with the major escape
/// ```
///
/// All three lines are from `gomod-m`, where 26 of the 174 requires are
/// pseudo-versions.
///
/// Checking the timestamp is well-formed, or that the hash is 12 hex digits,
/// would buy nothing this tool can act on — whether a pseudo-version resolves
/// is a question for the proxy, and stranger does not ask the network
/// anything. So: `v`, three numeric components, and everything after the first
/// `-` or `+` is the version author's business.
fn is_version(v: &str) -> bool {
    let Some(rest) = v.strip_prefix('v') else {
        return false;
    };
    let core = match rest.find(['-', '+']) {
        Some(at) => &rest[..at],
        None => rest,
    };
    let mut parts = core.split('.');
    let numeric =
        |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    numeric(parts.next())
        && numeric(parts.next())
        && numeric(parts.next())
        && parts.next().is_none()
        && rest[core.len()..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

fn syntax(what: impl Into<String>, line: u32, col: u32) -> Error {
    Error::Syntax {
        what: what.into(),
        line,
        col,
    }
}
