//! Names that look like a package a model invented.
//!
//! Edit distance on its own is not a rule. `lodash.merge` is two edits from
//! `lodash.mergewith` and both are real; so are `eslint-config-x` and
//! `eslint-configs-x`, and a registry the size of npm has thousands of these.
//! Any threshold loose enough to catch a typo is loose enough to catch a
//! legitimate sibling, and precision collapses.
//!
//! The clause that separates them is not about spelling at all:
//!
//! > A hallucinated package is a **root** dependency. Nothing depends on it,
//! > because nothing real has ever heard of it. A model put it in your
//! > manifest; no maintainer ever put it in theirs.
//!
//! `lodash.merge` is depended on by things. `lodahs` cannot be, because it
//! does not exist — the only reference to it in the world is the manifest
//! under audit. So the rule is a conjunction of three clauses, and the third
//! one carries most of the signal. `tests/ablation.rs` measures how much.

use crate::corpus;
use crate::lock::{Origin, Tree};
use crate::rules::{Finding, Rule, Severity};

#[derive(Debug, Clone, Copy)]
pub struct Config<'a> {
    /// The third clause. Off is not a supported mode of the tool — it exists
    /// so the ablation can measure what the clause is worth, and so the claim
    /// in the README is a number rather than an assertion.
    pub require_no_parent: bool,
    /// The name list to check against. `None` means the corpus compiled in for
    /// this ecosystem, which is what the binary always passes.
    ///
    /// It is a parameter because the corpus is this rule's largest assumption
    /// and an assumption you cannot vary is one you cannot measure. Every
    /// corpus is also incomplete — npm takes thousands of new names a day and
    /// mine is a snapshot from one afternoon — so the interesting question is
    /// not how the rule behaves with a perfect list. It is how it behaves when
    /// clause one starts failing, which is the condition it will actually meet.
    pub corpus: Option<&'a [&'a str]>,
}

impl Default for Config<'_> {
    fn default() -> Self {
        Config {
            require_no_parent: true,
            corpus: None,
        }
    }
}

pub fn scan(tree: &Tree, cfg: Config<'_>) -> Vec<Finding> {
    let names = cfg.corpus.unwrap_or_else(|| corpus::names(tree.ecosystem));
    let in_degree = tree.in_degree();
    let mut findings = Vec::new();

    for (i, pkg) in tree.packages.iter().enumerate() {
        // Somebody in this repo wrote it. Not a stranger.
        if pkg.first_party {
            continue;
        }
        // The corpus is a sample of one public registry. A package pulled from
        // git, a private index or a direct URL never passed through it, so its
        // absence from the list is not evidence of anything — the list was
        // never asked. The Cargo reader found this the hard way: `slint` and
        // `sg` in the cargo-m fixture are real crates fetched from git, and
        // all three clauses fired on both.
        if pkg.origin == Origin::Elsewhere {
            continue;
        }
        // Clause one. Cheap, and it eliminates everything but a few dozen
        // names, which is what makes clause two affordable.
        if corpus::contains_in(names, tree.ecosystem, &pkg.name) {
            continue;
        }
        // Clause three, checked before clause two because it is a vector
        // index and clause two is a scan of the corpus.
        if cfg.require_no_parent && in_degree[i] > 0 {
            continue;
        }
        // Clause two.
        let Some((nearest, distance)) = corpus::nearest_in(names, tree.ecosystem, &pkg.name) else {
            continue;
        };

        let parent = if in_degree[i] == 0 {
            "root-only, no parent".to_string()
        } else {
            format!("{} parent(s)", in_degree[i])
        };

        findings.push(Finding {
            rule: Rule::Slopsquat,
            severity: Severity::Critical,
            package: pkg.name.clone(),
            version: pkg.version.clone(),
            detail: format!("not in corpus · d={distance} from \"{nearest}\" · {parent}"),
        });
    }

    findings.sort_by(|a, b| a.package.cmp(&b.package));
    findings
}
