//! Dependencies whose whole job is one expression.
//!
//! # The signal, and what it is not
//!
//! It is not size. A package-lock.json entry holds `version`, `resolved`,
//! `integrity`, `license`, `engines` and the dependency lists — there is no
//! unpacked size, no file count, no export list, no line count anywhere in the
//! format. All of that is in the tarball, the tarball is on the registry, and
//! stranger does not fetch. So this rule does not measure triviality, and
//! nothing it prints should be read as though it had. It recognises names,
//! using two signals that really are in the file:
//!
//! 1. **A hand-written list.** Two dozen packages whose published purpose is a
//!    single expression or a re-export of a builtin — `isarray`, `is-number`,
//!    `left-pad`, `object-assign`. Picked by hand from the well-known
//!    micro-package cases, which makes the boundary my judgement rather than a
//!    threshold. Against a registry holding millions of names, two dozen is
//!    nothing. That is the honest size of this clause and there is no version
//!    of it that is not a list somebody wrote.
//!
//! 2. **Shape.** A name that reads as a predicate (`is-…`, `has-…`, scope
//!    stripped) *and* that resolves no dependencies of its own. Both halves
//!    come out of the lockfile. The second half is what stops it firing on
//!    `is-glob`, `has-tostringtag` and the other predicates that turned out to
//!    need help.
//!
//! # How it is wrong
//!
//! Clause 2 has no idea how long a file is. `is-callable` is dozens of lines
//! of edge cases around one `typeof`; `is-docker` reads `/proc` and memoises
//! the answer. Both are predicate-shaped, both resolve nothing, both are
//! reported here, and neither is a one-liner. That is the false-positive mode,
//! and it is not the exception — it is a good share of what clause 2 finds on
//! a real tree. A hit is worth twenty seconds of attention, not a verdict.
//!
//! It under-reports at least as badly. `function-bind`, `wrappy` and
//! `util-deprecate` are in the same weight class as anything on the list and
//! are not on it, because I am not going to claim I read them. Clause 2 is
//! blind to any micro-package that depends on another micro-package (`once`
//! needs `wrappy`) and to every one that is not named like a predicate, which
//! is most of them.
//!
//! Low, deliberately. None of this is a vulnerability. It is a count of
//! publishers who can push straight into your build, for code you could have
//! inlined — `left-pad` and `event-stream` were both packages this size, so
//! the count is worth having. It is just never urgent.

use crate::lock::Tree;
use crate::rules::{Finding, Rule, Severity};

/// Byte-sorted, because `binary_search` goes quiet rather than loud when it is
/// not. Public so `tests/rules.rs` can assert the order instead of trusting
/// whoever last added a name.
pub const KNOWN: &[&str] = &[
    "array-flatten",
    "code-point-at",
    "es-errors",
    "gopd",
    "has-flag",
    "hasown",
    "inherits",
    "is-arrayish",
    "is-buffer",
    "is-even",
    "is-extendable",
    "is-negative-zero",
    "is-npm",
    "is-number",
    "is-obj",
    "is-odd",
    "is-plain-obj",
    "is-windows",
    "isarray",
    "isobject",
    "left-pad",
    "number-is-nan",
    "object-assign",
    "pad-left",
];

pub fn scan(tree: &Tree) -> Vec<Finding> {
    // Out-degree over resolved edges rather than the entry's own `dependencies`
    // map. An optional dependency npm declined to install has no entry to
    // resolve to and so no edge, and for this rule's purposes a package whose
    // dependencies all vanished that way is a leaf in the tree you actually
    // have. It also means first-party packages read as leaves — their edges
    // live in `roots` — which is fine, since they are skipped anyway.
    let mut out_degree = vec![0u32; tree.packages.len()];
    for &(from, _) in &tree.edges {
        out_degree[from] += 1;
    }

    let mut findings = Vec::new();
    for (i, pkg) in tree.packages.iter().enumerate() {
        if pkg.first_party {
            continue;
        }
        let bare = match pkg.name.rfind('/') {
            Some(slash) => &pkg.name[slash + 1..],
            None => pkg.name.as_str(),
        };

        let detail = if KNOWN.binary_search(&bare).is_ok() {
            "one expression, one publisher · inlining it removes an account from your build"
        } else if out_degree[i] == 0 && (bare.starts_with("is-") || bare.starts_with("has-")) {
            "predicate-shaped, resolves nothing · size not measured, see rule docs"
        } else {
            continue;
        };

        findings.push(Finding {
            rule: Rule::Trivial,
            severity: Severity::Low,
            package: pkg.name.clone(),
            version: pkg.version.clone(),
            detail: detail.to_string(),
        });
    }

    findings.sort_by(|a, b| (&a.package, &a.version).cmp(&(&b.package, &b.version)));
    // npm nests the same version of `is-extendable` under two different
    // parents in npm-xl. Two hooks would be two events, but two copies of one
    // expression are one fact, printed once. Drift is the rule that cares that
    // the copies exist.
    findings.dedup_by(|a, b| a.package == b.package && a.version == b.version);
    findings
}
