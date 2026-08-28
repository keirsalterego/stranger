//! What every lockfile reader produces, regardless of ecosystem.

pub mod npm;

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecosystem {
    Npm,
    PyPi,
    Crates,
    Go,
}

impl Ecosystem {
    pub fn as_str(self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::PyPi => "pypi",
            Ecosystem::Crates => "crates.io",
            Ecosystem::Go => "go",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: String,
    /// The lockfile's own key for this entry. For npm that is the install
    /// path, which is what edge resolution scopes against; for flat formats
    /// it is just the name.
    pub key: String,
    pub dev: bool,
    pub optional: bool,
    /// A workspace member or path dependency. Not a stranger — somebody in
    /// this repo wrote it — so it is excluded from findings. Without this,
    /// every monorepo scan is mostly noise.
    pub first_party: bool,
    /// Code runs at install time. We can say that it does; we cannot say what
    /// it does, because lockfileVersion 3 records the flag and not the script.
    pub install_script: bool,
    /// Whether an integrity field was recorded at all. Never whether it is
    /// correct — see README LIMITS.
    pub has_integrity: bool,
}

#[derive(Debug)]
pub struct Tree {
    pub ecosystem: Ecosystem,
    pub source: PathBuf,
    pub packages: Vec<Package>,
    /// Package-to-package edges only. Edges from the root manifest live in
    /// `roots` instead, and that separation is load-bearing: the slopsquat
    /// rule asks whether any real package depends on a name, and "the
    /// manifest an LLM wrote lists it" is not evidence that one does.
    pub edges: Vec<(usize, usize)>,
    pub roots: Vec<usize>,
}

impl Tree {
    pub fn in_degree(&self) -> Vec<u32> {
        let mut deg = vec![0u32; self.packages.len()];
        for &(_, to) in &self.edges {
            deg[to] += 1;
        }
        deg
    }

    pub fn direct(&self) -> usize {
        self.roots.len()
    }

    pub fn transitive(&self) -> usize {
        self.packages.len() - self.roots.len()
    }
}
