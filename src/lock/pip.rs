//! requirements.txt — pip's install-from-a-file format.
//!
//! This is not a lockfile. It is the input a resolver takes, not the answer it
//! gives, and it is read here because people commit it and then treat it as
//! one. Most of what follows is a consequence of that: there is no resolved
//! version unless somebody typed `==`, and there is no dependency information
//! at all.
//!
//! # The format is flat, and the slopsquat rule pays for it
//!
//! A requirements.txt records a list. There are no transitive entries, no
//! nesting, and nothing that says one line needs another, so every package
//! here is a root, `roots` is `0..packages.len()`, and **`edges` is left
//! empty on purpose**. That is not this reader giving up — the edges are not
//! in the file to read.
//!
//! Which makes the slopsquat rule's third clause — "nothing real depends on
//! this name" — **vacuous on this format**. Every package trivially has
//! in-degree 0, the clause eliminates nothing, and the rule degenerates to
//! not-in-corpus AND near-a-real-name. `tests/ablation.rs` exists precisely
//! because that pair is the half of the conjunction that was never trusted on
//! its own, so a pip scan is noisier than an npm scan by construction and no
//! amount of parsing care changes it.
//!
//! The upgrade is a different file, not a better reader: `poetry.lock` and
//! `uv.lock` both record the resolved graph, and both are already sitting in
//! `fixtures/`.
//!
//! # What is skipped
//!
//! `-r`/`-c` includes are not followed, `-e` editables are not packages, and
//! `--index-url` is discussed at the point where it gets dropped.
//!
//! A requirement that names a *location* rather than a project — `https://…`,
//! `git+ssh://…`, `./vendor/pkg-1.0.tar.gz` — is skipped too, and that one is
//! worth saying out loud because it used to abort the file. pip installs all
//! three, none of them carries a name the PyPI corpus could speak about, and
//! `pkg @ https://…` is the only spelling that keeps a real name in front of
//! the URL. Skipping the line and reading the rest is the same call
//! `lock::cargo` makes for a dependency naming an entry that is not in the
//! file, and for the same reason: one unusual line should not cost the other
//! two hundred requirements their audit.

use crate::error::{Error, Result};
use crate::lock::{Ecosystem, Origin, Package, Pin, Tree};
use std::path::Path;

pub fn read(path: &Path, src: &str) -> Result<Tree> {
    let mut packages = Vec::new();

    // Continuations are joined before anything else sees the text, which is
    // also the order pip uses — `join_lines` runs ahead of comment stripping.
    // Pieces are concatenated with nothing between them, again like pip: a
    // real file indents its continuations, so the whitespace that separates
    // the tokens is already in the text.
    //
    // ponytail: pip additionally lets a whole-line comment *terminate* a
    // continuation. Nobody writes that, and getting it wrong costs one
    // mis-joined line rather than a wrong graph. Match it if a real file ever
    // shows up that needs it.
    let mut buf = String::new();
    let mut opened: Option<u32> = None;

    for (i, raw) in src.lines().enumerate() {
        let n = i as u32 + 1;
        // The line number reported for a joined line is where the logical line
        // *started*, which is the line an editor should be sent to.
        let start = *opened.get_or_insert(n);
        match raw.strip_suffix('\\') {
            Some(head) => buf.push_str(head),
            None => {
                buf.push_str(raw);
                requirement(&buf, start, &mut packages)?;
                buf.clear();
                opened = None;
            }
        }
    }
    // A file whose last line ends in a backslash. Yield what accumulated
    // rather than dropping it on the floor.
    if let Some(start) = opened {
        requirement(&buf, start, &mut packages)?;
    }

    let roots = (0..packages.len()).collect();
    Ok(Tree {
        ecosystem: Ecosystem::PyPi,
        source: path.to_path_buf(),
        packages,
        edges: Vec::new(),
        roots,
        // The one false in the codebase. See the module header: the edges are
        // not in the file to read, so `stranger tree` says the file has no
        // graph rather than printing an in-degree nobody measured.
        records_edges: false,
    })
}

fn requirement(logical: &str, line: u32, out: &mut Vec<Package>) -> Result<()> {
    let body = strip_comment(logical);
    let text = body.trim();
    if text.is_empty() {
        return Ok(());
    }

    // Columns are counted on the logical line. For the overwhelming majority
    // of lines that is the physical line and the column is exact; for a joined
    // one it points at the start of the first piece, and the message quotes
    // the fragment so the rest is findable.
    let col = body.chars().take_while(|c| c.is_whitespace()).count() as u32 + 1;

    // ponytail: `-r base.txt` is not followed. An include means file IO,
    // relative-path resolution and cycle detection, and it quietly turns one
    // file's audit into a directory crawl. Point stranger at the other file.
    //
    // `--index-url` and `--extra-index-url` go out with the rest, and that is
    // an omission worth naming: an extra index is the dependency-confusion
    // vector, and a line adding one is more interesting than most of the
    // packages under it. It is dropped because there is nowhere honest to put
    // it. `Tree` holds packages, and this is a fact about the file; minting a
    // `Package` to carry it would put a lie in the package count and a fake
    // name in the report. The upgrade is a field on `Tree` and a `Rule::Index`
    // that reads it — a bigger change than a reader, and not this one.
    if text.starts_with('-') {
        return Ok(());
    }

    // Per-requirement options ride on the same logical line once continuations
    // are joined:
    //
    //     requests==2.31.0 \
    //         --hash=sha256:aaa \
    //         --hash=sha256:bbb
    //
    // so requirement and options are separated by token, not by line, and this
    // runs before the marker is cut off — pip's order puts the options after
    // the marker, and cutting at `;` first would throw the hashes away.
    //
    // The requirement's own tokens are then glued back with nothing between
    // them, which is lossless: PEP 508 makes whitespace inside a requirement
    // insignificant, so `flask [async] >= 3.0` and `flask[async]>=3.0` are the
    // same requirement and the second one comes apart cleanly.
    let mut spec = String::new();
    let mut options = false;
    let mut hashed = false;
    for token in text.split_whitespace() {
        options |= token.starts_with('-');
        if options {
            hashed |= token.starts_with("--hash");
        } else {
            spec.push_str(token);
        }
    }

    // The marker comes off before anything goes looking for a version. A
    // marker is expression syntax carrying the same operators a specifier
    // does — `; python_version < "3.10"` has a `<` in it — so a classifier
    // that runs first reads the marker as a range and reports the requirement
    // as unpinned when it is pinned. Order, not cleverness.
    let head = match spec.find(';') {
        Some(i) => &spec[..i],
        None => spec.as_str(),
    };

    // PEP 508 names are `[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?`. Taking the
    // leading run of those characters ends the name at whatever comes next —
    // `[` for extras, an operator, `@` for a direct reference — without having
    // to know yet which of the three it is.
    let end = head
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        .unwrap_or(head.len());
    let (name, rest) = head.split_at(end);

    // A URL, a VCS reference, or a path — `https://host/x.tar.gz`,
    // `git+https://…`, `./vendor/pkg-1.0.tar.gz`. There is no project name in
    // any of them to look up, so there is nothing here for the rules to say,
    // and refusing the file over one of these took the other requirements down
    // with it. See the module doc.
    //
    // The `/` is what tells them apart from a name: PEP 508 allows none in a
    // name, an extras group or a specifier, so the only requirement that
    // legitimately reaches a slash is `pkg @ https://…` — which announces
    // itself with the `@` first and keeps its name. Checked before the name is
    // validated, because `./vendor/…` fails that check for the wrong reason.
    if !rest.starts_with('@') && rest.contains('/') {
        return Ok(());
    }

    if !name.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return Err(Error::Syntax {
            what: format!("`{head}` does not begin with a package name"),
            line,
            col,
        });
    }

    // Extras only now, and only after the name: `flask[async]>=3.0` puts the
    // bracket group between the two, so it has to be removed before the
    // specifier is read or the classifier is handed a `[`. Which extras were
    // asked for is not recorded — an extra changes what gets installed
    // alongside, never which version of this name gets installed, and the
    // version is what this reader is for.
    let rest = match rest.strip_prefix('[') {
        Some(tail) => match tail.find(']') {
            Some(i) => &tail[i + 1..],
            None => {
                return Err(Error::Syntax {
                    what: format!("`{head}` has an unclosed `[` in its extras"),
                    line,
                    col,
                });
            }
        },
        None => rest,
    };

    let (pinned, version) = classify(rest).ok_or_else(|| Error::Syntax {
        what: format!("`{head}` is not a version specifier stranger understands"),
        line,
        col,
    })?;

    out.push(Package {
        // As written. `Pillow`, `python-dateutil` and `python_dateutil` are one
        // project to PyPI under PEP 503 but three different strings here, and
        // folding them at read time would have the report quote a name that is
        // not in the file. `corpus::normalize` folds at comparison time, which
        // is the only place it is needed.
        name: name.to_string(),
        // Only ever the version that will be installed. `>=1.26` is not that,
        // so it stays out of this field and lives in `pinned` instead.
        version,
        key: name.to_string(),
        // requirements.txt carries none of these. A dev/test split lives in a
        // second file whose name is not something to guess from; an
        // environment marker is a condition, not npm's failure-is-tolerated
        // `optional`; and a source distribution can run whatever setup.py
        // wants without this file recording that it will — which is a real
        // blind spot and worth saying rather than defaulting past.
        dev: false,
        optional: false,
        first_party: false,
        install_script: false,
        // `--hash=sha256:…` is the only integrity this format has, and it is
        // opt-in. Whether the digest matches is a different question, and
        // stranger does not download anything to answer it.
        has_integrity: hashed,
        // A `pkg @ https://host/x.whl` direct reference bypasses PyPI, so the
        // PyPI corpus has nothing to say about the name. Everything else in a
        // requirements.txt resolves through an index, and this reader does not
        // follow `--index-url`, so it cannot tell a private index from PyPI —
        // said in the module doc rather than guessed at here.
        //
        // Read off `rest` and not `spec`: the tokens were glued together with
        // nothing between them, so `spec` always opens with the package name
        // and never with the `@`. That made this arm unreachable and every
        // direct reference a registry package — `nunpy @ https://…/nunpy.whl`
        // came out with the same CRITICAL as the plain `nunpy==1.0`, which is
        // exactly the false positive `Origin` was added to stop.
        origin: if rest.starts_with('@') {
            Origin::Elsewhere
        } else {
            Origin::Registry
        },
        pinned,
    });
    Ok(())
}

/// pip's comment rule, which is `(^|\s+)#.*$` and not "cut at the first `#`".
/// The difference is load-bearing: `pkg @ https://host/x.zip#sha256=…` puts a
/// `#` in a URL fragment, and cutting at the first one truncates the URL into
/// something that still parses.
fn strip_comment(line: &str) -> &str {
    let mut after_space = true; // start of line counts as preceded by space
    for (i, c) in line.char_indices() {
        if c == '#' && after_space {
            return &line[..i];
        }
        after_space = c.is_whitespace();
    }
    line
}

/// Strictest first, so `min` over the clauses picks the tightest one.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Kind {
    Exact,
    Capped,
    Open,
    /// `!=1.5` cuts one version out of the range and bounds neither end of
    /// what is left, so a specifier made only of these constrains nothing —
    /// `numpy!=1.5` installs the same thing `numpy` does on every day but one.
    /// Grouped with `<` and `>` it came out Medium, one notch *safer* than the
    /// bare name it is a rounding error away from, and could pass a
    /// `--fail-on high` the bare name fails.
    ///
    /// Looser than `Open` and last in the ordering, so `>=1.0,!=1.5` is still
    /// a range: one end is written down there, which is the whole distinction
    /// `Open` carries.
    Excluded,
}

/// Longest operator first: `==` must not shadow `===`, `<` must not shadow
/// `<=`.
const OPS: &[(&str, Kind)] = &[
    ("===", Kind::Exact),
    ("==", Kind::Exact),
    ("~=", Kind::Capped),
    ("!=", Kind::Excluded),
    ("<=", Kind::Open),
    (">=", Kind::Open),
    ("<", Kind::Open),
    (">", Kind::Open),
];

/// `None` when the text is not a specifier at all, which is the caller's cue
/// to raise a syntax error with a line number.
fn classify(spec: &str) -> Option<(Pin, String)> {
    if spec.is_empty() {
        return Some((Pin::Unconstrained, String::new()));
    }
    // `pkg @ https://host/pkg.whl`. A direct reference names bytes rather than
    // a version, and the bytes at a URL are whatever the host serves next
    // time — so it is unconstrained in the sense the pinning rule means, and
    // there is no version to record.
    if spec.starts_with('@') {
        return Some((Pin::Unconstrained, String::new()));
    }

    let mut tightest = Kind::Excluded;
    let mut exact = String::new();

    for clause in spec.split(',') {
        let (op, kind) = OPS.iter().find(|(op, _)| clause.starts_with(op))?;
        let value = &clause[op.len()..];
        if !is_version(value) {
            return None;
        }
        // `==1.2.*` uses the exact-equality operator and is not one version —
        // it is every release of 1.2. Same shape as `~=1.2`, so same class.
        let kind = if *kind == Kind::Exact && value.contains('*') {
            Kind::Capped
        } else {
            *kind
        };
        if kind == Kind::Exact {
            exact = value.to_string();
        }
        tightest = tightest.min(kind);
    }

    Some(match tightest {
        Kind::Exact => (Pin::Exact, exact),
        Kind::Capped => (Pin::Compatible(spec.to_string()), String::new()),
        Kind::Open => (Pin::Range(spec.to_string()), String::new()),
        Kind::Excluded => (Pin::Unconstrained, String::new()),
    })
}

/// Loose enough for everything PEP 440 allows — epochs, local versions,
/// `.post1`, `1.2.*` — and tight enough to catch the two things that reach
/// here by accident: an empty version (`foo==`) and a `#` pip would not have
/// treated as a comment because no space preceded it (`foo==1.0#note`).
fn is_version(v: &str) -> bool {
    !v.is_empty()
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '*' | '+' | '!' | '-' | '_'))
}
