//! What a scan produces.

pub mod drift;
pub mod pinning;
pub mod scripts;
pub mod slopsquat;
pub mod trivial;

use crate::corpus;
use crate::lock::{Format, Tree};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Some(Severity::Low),
            "medium" => Some(Severity::Medium),
            "high" => Some(Severity::High),
            "critical" => Some(Severity::Critical),
            _ => None,
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Slopsquat,
    Trivial,
    InstallScript,
    Drift,
    Pinning,
}

impl Rule {
    /// Every rule, in report order.
    ///
    /// A list, and a list is exactly what `rank` and `applies_to` are matches
    /// to avoid — neither compiles for a sixth rule until it has been given an
    /// answer, and this array happily stays five long. It is the one place a
    /// sixth rule can be forgotten, so `all_five_rules_are_in_all` is what
    /// catches it.
    pub const ALL: [Rule; 5] = [
        Rule::Slopsquat,
        Rule::InstallScript,
        Rule::Trivial,
        Rule::Drift,
        Rule::Pinning,
    ];

    /// Whether this rule could have fired on this file at all.
    ///
    /// "No findings" and "this format records no such signal" print the same
    /// and are not the same claim, and only one of them is a statement about
    /// the project. `install_script` is hardcoded `false` in the poetry, uv,
    /// Cargo and pnpm readers because those four files do not record it, so
    /// `stranger scan poetry.lock` reported no install scripts on a tree it had
    /// never asked the question about — clean at every `--fail-on` level, with
    /// nothing in the output to say otherwise. README LIMITS said so honestly;
    /// a CI gate does not read the README.
    ///
    /// This is not a finding and must never become one. An absence of evidence
    /// is not evidence, which is the same rule clause 3 of the slopsquat scan
    /// exists to keep — it changes what the report says, never the exit code.
    ///
    /// A match, so a sixth rule does not compile until somebody has decided
    /// which files it can speak about.
    ///
    /// ponytail: the *format* half is not compiler-enforced the way the rule
    /// half is. An eighth reader can be added without revisiting these arms and
    /// will silently inherit whatever `Format::of` returns for it. The upgrade
    /// is a `records_install_scripts: bool` on `Tree` beside `records_edges`,
    /// which does force every reader to answer — it just means editing all
    /// seven readers, which is a bigger change than the bug.
    pub fn applies_to(self, tree: &Tree) -> bool {
        match self {
            // Clause one is "not in the corpus", and Go has no corpus:
            // proxy.golang.org publishes no ranked list of module paths. The
            // rule already returns early on this and the early return is the
            // honest behaviour — it is the report that was quiet about it.
            Rule::Slopsquat => !corpus::names(tree.ecosystem).is_empty(),

            // Only `package-lock.json` writes the flag down. pnpm 9 dropped
            // `requiresBuild` and did not replace it, `Cargo.lock` says
            // nothing about `build.rs`, and neither PyPI lock format nor
            // `go.mod` records whether a package runs code on install. Four of
            // the seven, which is why this rule needed the answer first.
            Rule::InstallScript => tree.format() == Some(Format::Npm),

            // A lockfile answers this trivially: it records one resolved
            // version, so every entry is `Pin::Exact` and there is nothing to
            // report. `requirements.txt` is the one file here that is a
            // manifest people commit and treat as a lockfile, so it is the one
            // that can carry a `>=1.26`. Note this is narrower than the rule's
            // own early return, which lets `poetry.lock` and `uv.lock` through
            // on ecosystem and then finds every entry exact.
            Rule::Pinning => tree.format() == Some(Format::Pip),

            // Both work off names and versions, which every format on the list
            // records. Nothing structurally stops either from firing on any of
            // the seven — drift is quiet on `go.mod` in practice because
            // minimal version selection leaves one version per module path,
            // but that is the file's content and not the format declining to
            // say, and "quiet because there is nothing there" is exactly the
            // claim these two are entitled to make.
            Rule::Trivial | Rule::Drift => true,
        }
    }

    /// Where this rule prints: threat, then waste, then what the file does not
    /// pin down. A name that may not exist and code that runs at install can
    /// hurt you; a one-line package is only weight; drift and missing pins are
    /// the lockfile declining to say what will actually be installed. Fixed, so
    /// a diff between two scans is a diff and not a reshuffle.
    ///
    /// Not severity order, and it never was. `Pinning` alone spans low to high
    /// inside one block, so a block has no single severity to sort by — sorting
    /// on the worst one present would slide `UNPINNED` up and down the report
    /// between two scans of the same project, which is the thing a fixed order
    /// exists to prevent.
    ///
    /// A match rather than a list of every rule, because the compiler cannot
    /// see a variant missing from a list and can see one missing from a match.
    /// A sixth rule does not build until it has been given a place in the
    /// report.
    pub fn rank(self) -> usize {
        match self {
            Rule::Slopsquat => 0,
            Rule::InstallScript => 1,
            Rule::Trivial => 2,
            Rule::Drift => 3,
            Rule::Pinning => 4,
        }
    }

    pub fn heading(self) -> &'static str {
        match self {
            Rule::Slopsquat => "HALLUCINATION RISK",
            Rule::Trivial => "TRIVIAL",
            Rule::InstallScript => "INSTALL SCRIPTS",
            Rule::Drift => "VERSION DRIFT",
            Rule::Pinning => "UNPINNED",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Rule::Slopsquat => "slopsquat",
            Rule::Trivial => "trivial",
            Rule::InstallScript => "install-script",
            Rule::Drift => "drift",
            Rule::Pinning => "pinning",
        }
    }
}

/// The rules this file gives no signal for, in report order.
///
/// Deliberately not the complement of "rules that fired". A rule that could
/// have fired and did not is a fact about the project; a rule that could not
/// is a fact about the file format, and the report has to say which one it is
/// looking at.
pub fn not_applicable(tree: &Tree) -> Vec<Rule> {
    Rule::ALL
        .into_iter()
        .filter(|r| !r.applies_to(tree))
        .collect()
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: Rule,
    pub severity: Severity,
    pub package: String,
    pub version: String,
    /// Rendered right of the package name. Says why this fired, in the terms
    /// the rule actually used, so a reader can disagree with it.
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ALL` is the one list a sixth rule can be left off, and everything that
    /// reads it — the report, the JSON, `not_applicable` — would go quiet
    /// about that rule rather than fail. `rank` is a match, so a sixth rule
    /// must be given rank 5; this then finds the gap.
    #[test]
    fn all_five_rules_are_in_all() {
        let mut ranks: Vec<usize> = Rule::ALL.iter().map(|r| r.rank()).collect();
        assert_eq!(ranks, (0..Rule::ALL.len()).collect::<Vec<_>>());
        ranks.dedup();
        assert_eq!(ranks.len(), Rule::ALL.len());
    }
}
