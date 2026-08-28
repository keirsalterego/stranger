//! Packages that run code when you install them.
//!
//! `npm install` runs a dependency's `preinstall`, `install` and `postinstall`
//! hooks as part of installing it — before your test suite, before your own
//! first line of code, with your environment and whatever your ssh agent is
//! holding. That is the whole argument for High: for these packages the gap
//! between "a name appeared in the lockfile" and "that name's code ran on this
//! machine" is one command, and no review step fits inside it.
//!
//! The rule cannot tell you what the code does, and the wording is built so it
//! never implies otherwise. lockfileVersion 3 records `"hasInstallScript":
//! true` and stops: not the body, not which of the three hooks, not even the
//! script's name. The body is in the tarball on the registry, and stranger
//! does not fetch. `esbuild` unpacking a platform binary and a package curling
//! a payload produce the identical line here. Reading that line as triage is
//! the mistake this comment exists to prevent.

use crate::lock::Tree;
use crate::rules::{Finding, Rule, Severity};

pub fn scan(tree: &Tree) -> Vec<Finding> {
    let mut findings: Vec<Finding> = tree
        .packages
        .iter()
        // The root project and the workspace members have install scripts too.
        // Those are your build, not a stranger's, and the npm reader has
        // already dropped the root entry.
        .filter(|p| p.install_script && !p.first_party)
        .map(|p| Finding {
            rule: Rule::InstallScript,
            severity: Severity::High,
            package: p.name.clone(),
            version: p.version.clone(),
            detail: "runs code at install time · lockfile records the flag, not the script"
                .to_string(),
        })
        .collect();

    // One finding per entry, not per name: a duplicated package is installed
    // twice and the hook runs twice. Version breaks the tie so the report is
    // stable across scans.
    findings.sort_by(|a, b| (&a.package, &a.version).cmp(&(&b.package, &b.version)));
    findings
}
