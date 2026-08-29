//! What a scan produces.

pub mod drift;
pub mod pinning;
pub mod scripts;
pub mod slopsquat;
pub mod trivial;

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
