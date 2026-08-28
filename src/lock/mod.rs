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

/// Lockfiles we know how to read, in the order we look for them.
pub const KNOWN: &[&str] = &["package-lock.json"];

/// Read one lockfile, dispatching on its name.
pub fn read(path: &std::path::Path) -> crate::error::Result<Tree> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| crate::error::Error::io(path.display().to_string(), e))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    // Suffix rather than equality, so a file kept as `npm-xl.package-lock.json`
    // still reads. Lockfiles get renamed the moment you collect more than one.
    if name.ends_with("package-lock.json") {
        npm::read(path, &src)
    } else {
        Err(crate::error::Error::usage(format!(
            "{name}: not a lockfile stranger knows. It reads: {}",
            KNOWN.join(", ")
        )))
    }
}

/// Every known lockfile directly inside `dir`.
///
/// Deliberately not recursive yet — a recursive walk that wanders into
/// `node_modules` and audits four hundred vendored lockfiles is worse than no
/// walk at all.
pub fn discover(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    KNOWN
        .iter()
        .map(|n| dir.join(n))
        .filter(|p| p.is_file())
        .collect()
}
