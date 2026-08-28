//! Requirements that do not name a version.
//!
//! An unpinned requirement is not a vulnerability. It is the mechanism by
//! which somebody else's vulnerability reaches you without anybody changing a
//! file — the compromised release ships, `pip install -r requirements.txt`
//! runs in CI, and the diff that introduced it is empty. Every published pip
//! supply-chain incident has this shape, and it is the reason a rule about
//! punctuation is worth writing at all.
//!
//! Only fires on PyPI. npm, cargo and go all record a resolved version, so
//! every entry they produce is `Pin::Exact` and there is nothing here to say;
//! firing on them would mean either a rule that never triggers or a rule that
//! has started guessing.

use crate::lock::{Ecosystem, Pin, Tree};
use crate::rules::{Finding, Rule, Severity};

pub fn scan(tree: &Tree) -> Vec<Finding> {
    if tree.ecosystem != Ecosystem::PyPi {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for pkg in &tree.packages {
        // Severity here is a ranking of how much of the future the line lets
        // in, and nothing above High: an unpinned dependency is a way to be
        // compromised later, not evidence of being compromised now. Critical
        // is reserved for slopsquat, where the finding is a name that should
        // not exist.
        let (severity, detail) = match &pkg.pinned {
            Pin::Exact => continue,

            // No bound in either direction. `pip install numpy` today and the
            // same command in March install different programs, and there is
            // nothing in the repository that records which one you tested.
            Pin::Unconstrained => (
                Severity::High,
                "no version specifier · resolves to whatever is newest at install time".to_string(),
            ),

            // `>=1.0` is the common case and it is open above: every release
            // the maintainer has not published yet already matches. `<2` and
            // `!=1.5` are open below instead, which is a smaller window but
            // the same class of answer — the file does not say what installs.
            // One notch under Unconstrained because at least one end is
            // written down.
            Pin::Range(spec) => (
                Severity::Medium,
                format!("{spec} · a range, so the file does not say what installs"),
            ),

            // `~=1.2` caps the major, so a hostile 2.0 cannot arrive. That is
            // a real reduction and it is why this is Low rather than Medium —
            // but it is not a pin: the compromised releases that actually
            // happened were patch releases of a package people already
            // trusted, and every one of those still matches.
            Pin::Compatible(spec) => (
                Severity::Low,
                format!("{spec} · capped at the major, still floats below the cap"),
            ),
        };

        findings.push(Finding {
            rule: Rule::Pinning,
            severity,
            package: pkg.name.clone(),
            version: pkg.version.clone(),
            detail,
        });
    }

    findings.sort_by(|a, b| a.package.cmp(&b.package));
    findings
}
