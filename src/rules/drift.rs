//! One name, installed at more than one version.
//!
//! npm deduplicates what it can and nests what it cannot: when two packages
//! want incompatible ranges of the same name, the loser gets its own copy at
//! `node_modules/parent/node_modules/name`. Those nested keys are not a quirk
//! of the file format, they *are* how the format spells duplication — 184 of
//! npm-xl's 1,390 entries are nested — so this rule needs no resolver, no
//! registry and no `node_modules` on disk. It is reading the answer npm
//! already wrote down.
//!
//! Medium, and the argument is about the next advisory rather than today.
//! Nothing is exploitable because `ansi-regex` is installed at both 5.0.1 and
//! 6.2.2. But when a CVE lands on that name, the bump you make in your own
//! manifest moves the copy your manifest reaches and leaves the other one
//! pinned by whoever nested it — the fix reads as done while the vulnerable
//! code is still on disk. Duplication is the thing that turns patching into a
//! negotiation. Not High, because there is no vulnerability here yet; not Low,
//! because it decides how much tomorrow costs.

use crate::lock::Tree;
use crate::rules::{Finding, Rule, Severity};
use crate::semver::Version;
use std::cmp::Ordering;
use std::collections::HashMap;

pub fn scan(tree: &Tree) -> Vec<Finding> {
    let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
    for pkg in &tree.packages {
        if pkg.first_party {
            continue;
        }
        by_name
            .entry(pkg.name.as_str())
            .or_default()
            .push(pkg.version.as_str());
    }

    let mut findings = Vec::new();
    for (name, mut versions) in by_name {
        // Byte order was fine for `dedup`, which only needs equal strings to
        // land next to each other, and wrong for the list it prints: 29 of the
        // 448 drift findings across the fixtures came out misordered, among
        // them `fs-extra 10.1.0, 11.3.1, 11.3.5, 7.0.1, 8.1.0, 9.1.0` and
        // three on the poisoned fixture the README demos. 10 sorts before 7
        // one character at a time.
        //
        // A version this parser cannot read sorts after every one it can,
        // rather than interleaving on byte order somewhere it has nothing to
        // say. Every one of the 977 version strings in these fixtures parses;
        // the arm is there for the PEP 440 spellings (`1.0.post1`) that a
        // Python lockfile is allowed to contain.
        versions.sort_by(|a, b| match (Version::parse(a), Version::parse(b)) {
            (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.cmp(b)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.cmp(b),
        });
        versions.dedup();
        if versions.len() < 2 {
            continue;
        }
        findings.push(Finding {
            rule: Rule::Drift,
            severity: Severity::Medium,
            package: name.to_string(),
            // One finding per name, so `version` has nothing single to hold
            // and the report prints the bare name. 76 names is something you
            // read; the 180 copies behind them are a wall.
            version: String::new(),
            detail: format!("{} versions: {}", versions.len(), versions.join(", ")),
        });
    }

    findings.sort_by(|a, b| a.package.cmp(&b.package));
    findings
}
