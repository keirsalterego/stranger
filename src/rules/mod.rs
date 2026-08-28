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

/// Report order. Worst first, and stable, so a diff between two scans is a
/// diff and not a reshuffle.
pub const ORDER: &[Rule] = &[
    Rule::Slopsquat,
    Rule::InstallScript,
    Rule::Trivial,
    Rule::Drift,
    Rule::Pinning,
];

impl Rule {
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
