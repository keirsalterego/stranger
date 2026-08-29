//! Known-real package names, one sorted list per ecosystem.
//!
//! The lists are compiled into the binary with `include_str!`. That is the
//! whole reason `stranger` works on a plane: there is no fetch, no cache
//! directory, and no "corpus not found" failure mode. The three lists are
//! 2,960,053 bytes of text in a 4,064,792-byte release binary — nearly three
//! quarters of the binary is
//! corpus — and that buys the tool's central claim.
//!
//! Names are stored pre-normalised and pre-sorted in *byte* order, because
//! that is the order `str`'s `Ord` uses and therefore the order
//! `binary_search` needs. Shell `sort` is locale-dependent and will happily
//! produce a different one, so `tests/corpus.rs` asserts sortedness rather
//! than trusting whoever last regenerated the files.

use crate::distance;
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
        // Empty, and reached. `lock::gomod` reads `go.mod` now, so every Go
        // scan comes through here and gets nothing — proxy.golang.org
        // publishes no ranked list, and a module path is a domain, so edit
        // distance over them is a different problem that a list nobody
        // publishes cannot answer. Parsing the file and having no corpus for
        // it are separate facts and both are true.
        //
        // The rule does not merely come out quiet by arithmetic: an empty
        // corpus stops `slopsquat::scan` at its first line, so a Go tree is
        // skipped by decision rather than by an accident of an empty list.
        // Said out loud in README LIMITS too.
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

/// `contains_in` against the compiled-in list.
///
/// Nothing in `src/` calls this — the rules take their list as a parameter so
/// `tests/ablation.rs` can shrink it. Its one caller is `tests/pip.rs`, which
/// is asking about the corpus itself rather than about a rule, and that is the
/// only question this signature can answer.
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

/// The closest real name in `names` to `name`, if any is close enough.
///
/// One-shot: builds an index, asks it one question, throws it away. A caller
/// with more than one name to ask about should build a [`ByLength`] and keep
/// it, which is what `rules::slopsquat` does.
pub fn nearest_in<'a>(names: &[&'a str], eco: Ecosystem, name: &str) -> Option<(&'a str, usize)> {
    ByLength::new(names).nearest(eco, name)
}

/// The corpus, grouped by name length.
///
/// A name at edit distance `k` cannot differ in length by more than `k`, so a
/// sweep only ever has to look at the `2k + 1` buckets around the query and
/// never touches the rest of the list. That bound is exact, not a heuristic,
/// and `tests/corpus.rs` holds it to an exhaustive sweep rather than to my
/// word for it.
///
/// The cost of not exploiting it was not theoretical: 500 names absent from
/// the npm corpus took 19.1s of wall clock to look up, where 500 names present
/// in it cost the rule nothing at all, because clause one answers those from a
/// binary search. Buckets take one unknown name from 23.7 to 19.7 CPU
/// milliseconds — the rest of that gap is `distance::Query`, and the numbers
/// are on it. A flat format has no edges, so clause three gates nothing and
/// every package in a `requirements.txt` reaches the sweep; and the corpus is
/// a snapshot, so the absent set grows every day it is not regenerated.
///
/// ponytail: a `Vec<Vec<&str>>` filled by one pass, and nothing cleverer. The
/// ceiling is the widest band — npm holds 7,075 thirteen-character names and
/// 33,316 across eleven to fifteen, so a thirteen-character query still runs
/// the table against a quarter of the list. The upgrade past that is a
/// deletion-neighbourhood index, which wants more memory than a corpus already
/// compiled into the binary can spare.
pub struct ByLength<'a> {
    buckets: Vec<Vec<&'a str>>,
}

impl<'a> ByLength<'a> {
    pub fn new(names: &[&'a str]) -> ByLength<'a> {
        let longest = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
        let mut buckets = vec![Vec::new(); longest + 1];
        for &name in names {
            buckets[name.chars().count()].push(name);
        }
        ByLength { buckets }
    }

    pub fn nearest(&self, eco: Ecosystem, name: &str) -> Option<(&'a str, usize)> {
        let normalized = normalize(eco, name);
        let mut query = distance::Query::new(&normalized);
        let k = query.budget();
        // A name too short to have earned an edit is not a near-miss of
        // anything, and the sweep is skipped rather than run and thrown away.
        if k == 0 {
            return None;
        }
        let lo = query.char_len().saturating_sub(k);
        let hi = (query.char_len() + k).min(self.buckets.len() - 1);
        // A query longer than anything in the corpus, so `lo` is past the end.
        let buckets = self.buckets.get(lo..=hi)?;

        let mut best: Option<(&'a str, usize)> = None;
        for &candidate in buckets.iter().flatten() {
            let Some(d) = query.distance_to(candidate) else {
                continue;
            };
            // Ties go to the shorter name, which is almost always the real
            // package and the typo's parent — `lodash` over `lodash.merge` —
            // and then to the byte-order-first, which is what the flat scan
            // over a sorted list used to give for free. Spelling it out keeps
            // the answer from depending on which bucket the sweep reached
            // first.
            if best.is_none_or(|(b, bd)| (d, candidate.len(), candidate) < (bd, b.len(), b)) {
                best = Some((candidate, d));
            }
        }
        best
    }
}
