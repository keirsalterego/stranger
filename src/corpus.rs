//! Known-real package names, one sorted list per ecosystem.
//!
//! The lists are compiled into the binary with `include_str!`. That is the
//! whole reason `stranger` works on a plane: there is no fetch, no cache
//! directory, and no "corpus not found" failure mode. The three lists are
//! 2,960,053 bytes of text in a 3.6 MB release binary — most of the binary is
//! corpus — and that buys the tool's central claim.
//!
//! Names are stored pre-normalised and pre-sorted in *byte* order, because
//! that is the order `str`'s `Ord` uses and therefore the order
//! `binary_search` needs. Shell `sort` is locale-dependent and will happily
//! produce a different one, so `tests/corpus.rs` asserts sortedness rather
//! than trusting whoever last regenerated the files.

use crate::distance::{self, MAX_EDIT_DISTANCE};
use crate::lock::Ecosystem;
use std::sync::LazyLock;

static NPM: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("../corpus/npm.txt").lines().collect());
static PYPI: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("../corpus/pypi.txt").lines().collect());
static CRATES: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("../corpus/crates-io.txt").lines().collect());

pub fn names(eco: Ecosystem) -> &'static [&'static str] {
    match eco {
        Ecosystem::Npm => &NPM,
        Ecosystem::PyPi => &PYPI,
        Ecosystem::Crates => &CRATES,
        // proxy.golang.org publishes no ranked list and module paths are
        // domains, so edit distance over them is a different problem. go.mod
        // still parses; this rule just never fires on it. Said out loud in
        // README LIMITS rather than shipped as a rule that silently does
        // nothing.
        Ecosystem::Go => &[],
    }
}

/// PyPI treats `Foo.Bar`, `foo-bar` and `foo_bar` as one project (PEP 503),
/// and a requirements.txt will contain any of them. Normalising both sides
/// keeps a separator choice from reading as a one-character typo.
///
/// npm names are already lowercase by registry rule. crates.io keeps `_` and
/// `-` distinct in display even though it refuses to register both, so they
/// are left alone and the edit distance covers the confusion.
pub fn normalize(eco: Ecosystem, name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if eco != Ecosystem::PyPi {
        return lower;
    }
    let mut out = String::with_capacity(lower.len());
    let mut last_was_sep = false;
    for c in lower.chars() {
        let sep = matches!(c, '-' | '_' | '.');
        if sep {
            if !last_was_sep {
                out.push('-');
            }
        } else {
            out.push(c);
        }
        last_was_sep = sep;
    }
    out
}

pub fn contains(eco: Ecosystem, name: &str) -> bool {
    contains_in(names(eco), eco, name)
}

/// Membership against an explicit list.
///
/// The list is a parameter and not a global because the corpus is the rule's
/// biggest assumption, and an assumption you cannot vary is one you cannot
/// measure. `tests/ablation.rs` shrinks it deliberately to find out which
/// clause is holding the rule up.
pub fn contains_in(names: &[&str], eco: Ecosystem, name: &str) -> bool {
    let name = normalize(eco, name);
    names.binary_search(&name.as_str()).is_ok()
}

/// The closest real name within `MAX_EDIT_DISTANCE`, if there is one.
///
/// ponytail: linear scan over the whole list. It looks wrong and is not: this
/// only ever runs for names that already failed the `contains` check, which on
/// the fixtures is a couple of dozen out of 1,390. The length filter inside
/// `distance::within` rejects most of the corpus before any table is
/// allocated. If the not-in-corpus set ever gets large, bucket the corpus by
/// length — the ordering by name is not doing any work for this query.
pub fn nearest(eco: Ecosystem, name: &str) -> Option<(&'static str, usize)> {
    nearest_in(names(eco), eco, name)
}

pub fn nearest_in<'a>(names: &[&'a str], eco: Ecosystem, name: &str) -> Option<(&'a str, usize)> {
    let query = normalize(eco, name);
    names
        .iter()
        .filter_map(|&candidate| {
            distance::within(&query, candidate, MAX_EDIT_DISTANCE).map(|d| (candidate, d))
        })
        // Ties go to the shorter name, which is almost always the real package
        // and the typo's parent — `lodash` over `lodash.merge`.
        .min_by_key(|&(candidate, d)| (d, candidate.len()))
}
