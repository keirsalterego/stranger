//! `stranger diff old new` — what changed between two lockfiles.
//!
//! `scan` answers "is this tree bad". A reviewer looking at a pull request has
//! a narrower question that `scan` cannot answer: *did this change make it
//! worse*. A repository with 211 trivial packages has 211 of them before and
//! after, and a gate that fires on the total fires on every pull request until
//! somebody turns it off. A gate that fires on what the diff *added* fires on
//! the pull request that added something.
//!
//! So `--fail-on` means something different here, and deliberately: on `scan`
//! it is the worst finding in the tree, on `diff` it is the worst finding this
//! change introduced. A pull request that adds nothing exits 0 on a tree that
//! `scan --fail-on high` would fail, and that is the intended reading rather
//! than a hole.
//!
//! # Findings are matched by rule and package, not by version
//!
//! A bumped dependency keeps its findings. `left-pad@1.0.0` with an install
//! script becomes `left-pad@1.1.0` with an install script, and keying the
//! comparison on `(rule, name, version)` would report that as one finding
//! fixed and one introduced — two lines of noise for a change that altered
//! nothing about the risk. Keying on `(rule, name)` reports it as neither.
//!
//! The cost is real and worth stating: a package already flagged for one rule
//! can change version and the new version's finding is not called new. What
//! that hides is a *version* change on an already-flagged package, which the
//! `changed` list prints anyway, two blocks up.

use crate::lock::{self, Tree};
use crate::rules::{Finding, Severity, drift, pinning, scripts, slopsquat, trivial};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub struct Diff {
    pub old: Tree,
    pub new: Tree,
    /// `name@version` present in the new tree and not the old.
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// `(name, from, to)` for a name in both trees at different versions.
    ///
    /// A name at several versions on either side contributes one row per
    /// version that arrived or left, because "lodash 4.17.20 -> 4.17.21" is a
    /// claim about a tree that holds exactly one lodash.
    pub changed: Vec<(String, String, String)>,
    pub introduced: Vec<Finding>,
    pub resolved: Vec<Finding>,
}

impl Diff {
    /// The worst thing this change introduced, which is what `--fail-on` gates
    /// on. `None` when it introduced nothing.
    pub fn worst(&self) -> Option<Severity> {
        self.introduced.iter().map(|f| f.severity).max()
    }

    /// Nothing to report — which has to include the findings and not just the
    /// package lists, because `report::diff_human` prints nothing at all when
    /// this is true while `--fail-on` gates on [`worst`](Diff::worst).
    ///
    /// The two disagree whenever a finding moves without a package moving,
    /// and that is not hypothetical: one tree read through two formats that
    /// record different things — npm records install scripts, yarn v1 does
    /// not — has identical `added`, `removed` and `changed` and a different
    /// finding set. Counting only the package lists printed "no change to the
    /// dependency tree" and exited 1, which is the worst pair of things a CI
    /// gate can do at the same time.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.introduced.is_empty()
            && self.resolved.is_empty()
    }
}

pub fn compare(old_path: &Path, new_path: &Path) -> crate::error::Result<Diff> {
    let old = lock::read(old_path)?;
    let new = lock::read(new_path)?;
    // A Cargo.lock against a package-lock.json parses fine and produces a
    // diff in which every package was added and every package was removed —
    // a confident, detailed, entirely meaningless answer. The two files were
    // almost certainly named in the wrong order or the wrong pair, and there
    // is no reading of "what changed" that spans two registries.
    //
    // Two npm formats *are* comparable: a project that moved from npm to pnpm
    // has the same packages under a different reader, and that diff is the one
    // somebody migrating actually wants.
    if old.ecosystem != new.ecosystem {
        return Err(crate::error::Error::usage(format!(
            // `as_str`, not `{:?}`: the Debug name is `Crates` and every other
            // thing this tool prints — the JSON `ecosystem` field included —
            // says `crates.io`. An error is not the place to introduce a
            // second name for the registry the user just named.
            "{} and {} are different ecosystems ({} and {}); there is nothing to compare",
            old_path.display(),
            new_path.display(),
            old.ecosystem.as_str(),
            new.ecosystem.as_str(),
        )));
    }
    Ok(build(old, new))
}

fn build(old: Tree, new: Tree) -> Diff {
    let old_ids = identities(&old);
    let new_ids = identities(&new);

    let added: Vec<String> = new_ids.difference(&old_ids).cloned().collect();
    let removed: Vec<String> = old_ids.difference(&new_ids).cloned().collect();

    // A name is "changed" rather than added-and-removed when it is in both
    // trees. Version sets rather than a single version, because npm trees
    // routinely hold the same name three times and picking one to report would
    // be picking arbitrarily.
    let old_versions = versions(&old);
    let new_versions = versions(&new);
    let mut changed = Vec::new();
    for (name, before) in &old_versions {
        let Some(after) = new_versions.get(name) else {
            continue;
        };
        if before == after {
            continue;
        }
        let gone: Vec<&str> = before.difference(after).map(String::as_str).collect();
        let arrived: Vec<&str> = after.difference(before).map(String::as_str).collect();
        // Zipped, so a one-for-one swap reads as one row. An uneven change —
        // two versions collapsing to one — pads with the other side's whole
        // list rather than inventing a pairing.
        let rows = gone.len().max(arrived.len());
        for i in 0..rows {
            changed.push((
                name.clone(),
                gone.get(i).copied().unwrap_or("—").to_string(),
                arrived.get(i).copied().unwrap_or("—").to_string(),
            ));
        }
    }

    let old_findings = findings(&old);
    let new_findings = findings(&new);
    // Keyed on the rule's *name* rather than the enum. `Rule` derives no
    // `Ord`, and deriving one here would order by declaration while `rank()`
    // orders by report position — two orderings on one type, disagreeing the
    // moment somebody reorders the enum. The name is already the stable
    // identifier the JSON output uses.
    let key = |f: &Finding| (f.rule.id(), f.package.clone());
    let old_keys: BTreeSet<(&str, String)> = old_findings.iter().map(key).collect();
    let new_keys: BTreeSet<(&str, String)> = new_findings.iter().map(key).collect();

    let introduced: Vec<Finding> = new_findings
        .iter()
        .filter(|f| !old_keys.contains(&key(f)))
        .cloned()
        .collect();
    let resolved: Vec<Finding> = old_findings
        .iter()
        .filter(|f| !new_keys.contains(&key(f)))
        .cloned()
        .collect();

    let mut added = added;
    let mut removed = removed;
    added.sort_unstable();
    removed.sort_unstable();
    changed.sort_unstable();

    Diff {
        old,
        new,
        added,
        removed,
        changed,
        introduced,
        resolved,
    }
}

/// Every rule, in report order — the same set `scan` runs, so a finding cannot
/// be introduced according to one command and absent according to the other.
fn findings(tree: &Tree) -> Vec<Finding> {
    let mut out = slopsquat::scan(tree, slopsquat::Config::default());
    out.extend(scripts::scan(tree));
    out.extend(trivial::scan(tree));
    out.extend(drift::scan(tree));
    out.extend(pinning::scan(tree));
    out.sort_by_key(|f| f.rule.rank());
    out
}

/// `name@version` for everything that is not the project's own code. A
/// workspace member appearing or leaving is a change to the repository, not to
/// its exposure to strangers.
fn identities(tree: &Tree) -> BTreeSet<String> {
    tree.packages
        .iter()
        .filter(|p| !p.first_party)
        .map(|p| format!("{}@{}", p.name, p.version))
        .collect()
}

fn versions(tree: &Tree) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in tree.packages.iter().filter(|p| !p.first_party) {
        out.entry(p.name.clone())
            .or_default()
            .insert(p.version.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::{Ecosystem, Origin, Package, Pin};
    use std::path::PathBuf;

    fn pkg(name: &str, version: &str) -> Package {
        Package {
            name: name.to_string(),
            version: version.to_string(),
            key: format!("{name}@{version}"),
            dev: false,
            optional: false,
            first_party: false,
            install_script: false,
            has_integrity: true,
            pinned: Pin::Exact,
            origin: Origin::Registry,
        }
    }

    fn tree(packages: Vec<Package>) -> Tree {
        Tree {
            ecosystem: Ecosystem::Npm,
            source: PathBuf::from("package-lock.json"),
            packages,
            edges: Vec::new(),
            roots: Vec::new(),
            records_edges: true,
            records_install_scripts: true,
        }
    }

    #[test]
    fn added_and_removed() {
        let d = build(
            tree(vec![pkg("a", "1.0.0"), pkg("b", "1.0.0")]),
            tree(vec![pkg("a", "1.0.0"), pkg("c", "2.0.0")]),
        );
        assert_eq!(d.added, ["c@2.0.0"]);
        assert_eq!(d.removed, ["b@1.0.0"]);
        assert!(d.changed.is_empty());
    }

    /// A bump is one row, not an add and a remove.
    #[test]
    fn a_bump_is_a_change() {
        let d = build(
            tree(vec![pkg("lodash", "4.17.20")]),
            tree(vec![pkg("lodash", "4.17.21")]),
        );
        assert_eq!(
            d.changed,
            [(
                "lodash".to_string(),
                "4.17.20".to_string(),
                "4.17.21".to_string()
            )]
        );
        // Still in `added`/`removed` by identity, because the identity really
        // did change; `changed` is the readable view of the same fact.
        assert_eq!(d.added, ["lodash@4.17.21"]);
    }

    #[test]
    fn an_unchanged_tree_diffs_to_nothing() {
        let d = build(tree(vec![pkg("a", "1.0.0")]), tree(vec![pkg("a", "1.0.0")]));
        assert!(d.is_empty());
        assert!(d.introduced.is_empty());
        assert!(d.worst().is_none());
    }

    /// The gate: a package already flagged before the change does not count as
    /// introduced just because its version moved.
    #[test]
    fn a_bump_does_not_reintroduce_a_finding() {
        let mut before = pkg("esbuild", "0.20.0");
        before.install_script = true;
        let mut after = pkg("esbuild", "0.21.0");
        after.install_script = true;
        let d = build(tree(vec![before]), tree(vec![after]));
        assert!(d.introduced.is_empty(), "{:?}", d.introduced);
        assert!(d.resolved.is_empty());
        assert_eq!(d.changed.len(), 1);
    }

    #[test]
    fn a_new_install_script_is_introduced() {
        let mut after = pkg("esbuild", "0.21.0");
        after.install_script = true;
        let d = build(tree(vec![pkg("a", "1.0.0")]), tree(vec![after]));
        assert_eq!(d.introduced.len(), 1);
        assert_eq!(d.introduced[0].package, "esbuild");
        assert_eq!(d.worst(), Some(d.introduced[0].severity));
    }

    #[test]
    fn removing_the_problem_resolves_it() {
        let mut before = pkg("esbuild", "0.21.0");
        before.install_script = true;
        let d = build(tree(vec![before]), tree(vec![pkg("a", "1.0.0")]));
        assert_eq!(d.resolved.len(), 1);
        assert!(d.introduced.is_empty());
        assert!(d.worst().is_none());
    }

    /// The prose and the exit code must agree. One tree read through two
    /// formats: yarn v1 does not record install scripts and npm does, so the
    /// package lists are identical and the finding set is not. `is_empty` has
    /// to see that, or `diff` prints "no change to the dependency tree" and
    /// exits 1 in the same breath — a red build with nothing on screen.
    #[test]
    fn a_finding_moving_alone_is_not_an_empty_diff() {
        let quiet = tree(vec![pkg("esbuild", "0.21.0")]);
        let mut loud = tree(vec![pkg("esbuild", "0.21.0")]);
        loud.packages[0].install_script = true;

        let d = build(quiet, loud);
        assert!(d.added.is_empty() && d.removed.is_empty() && d.changed.is_empty());
        assert_eq!(d.introduced.len(), 1);
        assert!(d.worst().is_some());
        assert!(!d.is_empty(), "a diff that fails --fail-on must print why");
    }

    /// Two versions collapsing to one pads rather than inventing a pairing.
    #[test]
    fn an_uneven_change_pads() {
        let d = build(
            tree(vec![pkg("x", "1.0.0"), pkg("x", "2.0.0")]),
            tree(vec![pkg("x", "3.0.0")]),
        );
        assert_eq!(d.changed.len(), 2);
        assert_eq!(d.changed[1].2, "—");
    }
}
