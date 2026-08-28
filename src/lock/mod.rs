//! What every lockfile reader produces, regardless of ecosystem.

pub mod cargo;
pub mod npm;
pub mod pip;
pub mod pypi;

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

/// How tightly the file constrains the version.
///
/// A lockfile answers this trivially — it records one resolved version, so
/// every entry is `Exact`. A manifest that gets committed and treated as a
/// lockfile does not, and `requirements.txt` is the one people commit.
///
/// The non-exact variants carry the specifier as written because the finding
/// has to be arguable: "`>=1.26`" is something a reader can check against the
/// file, "unpinned" is something they have to take on trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    /// One version, and it is the one in `Package::version`.
    Exact,
    /// `~=1.2`, or `==1.2.*`. Capped, and floats below the cap.
    Compatible(String),
    /// `>=1.0`, `<2`, `!=1.5`, or several joined by commas. Open-ended in at
    /// least one direction.
    Range(String),
    /// No specifier at all. Whatever the index serves that day.
    Unconstrained,
}

/// Where a package came from, to the extent the lockfile records it.
///
/// This exists because of a false positive the Cargo reader found. `slint` and
/// `sg` in the `cargo-m` fixture are real crates pulled straight from git. They
/// never went through crates.io, so they cannot be in a crates.io corpus; only
/// workspace members reference them, so nothing depends on them either. All
/// three slopsquat clauses fire on a package that is entirely legitimate.
///
/// The corpus can only speak about the public registry. Asking it about a git
/// URL or a private index is asking a question it has no way to answer, and
/// treating "absent from a list that never covered it" as evidence is the bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The ecosystem's public registry — the one the corpus is a sample of.
    Registry,
    /// git, a path, a private index, a direct URL. The corpus has nothing to
    /// say, so the name rules stay quiet.
    Elsewhere,
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
    pub pinned: Pin,
    pub origin: Origin,
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

    /// Packages reached only through another package.
    ///
    /// First-party entries are excluded from both ends of this. A workspace
    /// member is neither a direct dependency nor a transitive one — it is your
    /// own code, and counting the 14 in the npm-xl fixture as transitive
    /// dependencies overstates the tree by 14 packages you wrote.
    pub fn transitive(&self) -> usize {
        self.third_party().saturating_sub(self.roots.len())
    }

    /// How many packages came from somewhere else. This is the number the
    /// report leads with, because it is the one the tool is about.
    pub fn third_party(&self) -> usize {
        self.packages.iter().filter(|p| !p.first_party).count()
    }

    pub fn workspace_members(&self) -> usize {
        self.packages.len() - self.third_party()
    }
}

/// Lockfiles we know how to read, in the order we look for them.
pub const KNOWN: &[&str] = &[
    "package-lock.json",
    "Cargo.lock",
    "requirements.txt",
    "poetry.lock",
    "uv.lock",
];

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
    //
    // Suffix matching only stays unambiguous while no known name is a suffix
    // of another. None of these three is, so the arm order is documentation
    // rather than precedence — but check it before adding a fourth.
    if name.ends_with("package-lock.json") {
        npm::read(path, &src)
    } else if name.ends_with("Cargo.lock") {
        cargo::read(path, &src)
    } else if name.ends_with("requirements.txt") {
        pip::read(path, &src)
    } else if name.ends_with("poetry.lock") {
        pypi::poetry(path, &src)
    } else if name.ends_with("uv.lock") {
        pypi::uv(path, &src)
    } else {
        Err(crate::error::Error::usage(format!(
            "{name}: not a lockfile stranger knows. It reads: {}",
            KNOWN.join(", ")
        )))
    }
}

/// Every known lockfile under `dir`.
///
/// The walk is what makes this usable on a monorepo, and the skip list in
/// `crate::walk` is what keeps it from wandering into `node_modules` and
/// auditing four hundred vendored lockfiles belonging to other people.
pub fn discover(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    crate::walk::lockfiles(dir, KNOWN)
}
