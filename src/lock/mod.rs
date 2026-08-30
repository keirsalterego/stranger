//! What every lockfile reader produces, regardless of ecosystem.

pub mod cargo;
pub mod gomod;
pub mod npm;
pub mod pip;
pub mod pnpm;
pub mod pypi;
pub mod yarn;

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
    /// path — `node_modules/a/node_modules/b`, the nesting that makes this a
    /// second copy — for pnpm `name@version`, for Cargo `name version`, and
    /// for the flat formats just the name.
    ///
    /// Not what edge resolution scopes against, which is what this comment
    /// used to claim: `npm::resolve` walks the entries map by its keys and
    /// never looks at this field. It is here for `stranger tree`, which prints
    /// it when it says something the name and version do not.
    pub key: String,
    /// Declared for development only.
    ///
    /// npm records it per entry and `poetry.lock` records it per group; pnpm
    /// 9, Cargo.lock, requirements.txt and go.mod record nothing this maps
    /// onto, and those readers leave it false rather than guessing. So `false`
    /// means "not marked dev", never "not dev", and nothing aggregates it —
    /// `stranger tree` prints the flag where a reader set it and says nothing
    /// where none did, which is the only claim the four silent formats support.
    pub dev: bool,
    /// Installed only if the platform or the peer graph allows it. npm records
    /// it on the entry, pnpm on the snapshot; the same caveat as `dev` applies
    /// to the four formats that record neither.
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
    /// Whether this file's *format* records dependency edges at all.
    ///
    /// `requirements.txt` does not — it is a list, not a graph — so an empty
    /// `edges` there means "the file does not say", while an empty `edges` on
    /// a `poetry.lock` means "nothing here depends on anything". `stranger
    /// tree` has to tell those apart: reporting in-degree 0 on a flat file
    /// would dress an absence of evidence up as evidence, which is the exact
    /// mistake clause 3 exists to avoid.
    ///
    /// A field rather than a match on the filename somewhere, so a seventh
    /// reader does not compile until it has answered the question.
    pub records_edges: bool,
    /// Whether this file records that a package runs code at install time.
    ///
    /// Not derivable from the format any more, which is why it is a field and
    /// not a `match` in `Rule::applies_to`: `package-lock.json` records it,
    /// pnpm records it at lockfileVersion 6 and dropped it at 9, and the same
    /// filename covers both. Every other reader answers `false`, and `false`
    /// here means the file was never asked — never that the answer was no.
    pub records_install_scripts: bool,
}

impl Tree {
    /// Which reader produced this.
    ///
    /// Derived from `source` rather than stored on the struct, because storing
    /// it means a field every reader has to fill in and the readers belong to
    /// other people this week. `None` is unreachable through `read`, which
    /// only builds a `Tree` after `Format::of` said yes — a test constructing
    /// one by hand can still get it, and gets a rule that declines to claim
    /// anything rather than a panic.
    pub fn format(&self) -> Option<Format> {
        self.source
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(Format::of)
    }

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

    /// Third-party entries that recorded an integrity field.
    ///
    /// Presence, never correctness. Verifying a `sha512-...` needs SHA-512 and
    /// Rust std has no crypto at all, so the honest half of the answer is the
    /// only half there is — and reporting it is still worth more than the
    /// field being read by six readers and looked at by nobody, which is what
    /// it was. First-party entries are excluded because npm does not record an
    /// integrity for a workspace member, so counting them would put a floor
    /// under the number that has nothing to do with the registry.
    pub fn with_integrity(&self) -> usize {
        self.packages
            .iter()
            .filter(|p| !p.first_party && p.has_integrity)
            .count()
    }
}

/// Lockfiles we know how to read, in the order we look for them.
pub const KNOWN: &[&str] = &[
    "package-lock.json",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "requirements.txt",
    "poetry.lock",
    "uv.lock",
    "go.mod",
    "yarn.lock",
];

/// Which file this is, as opposed to which registry it points at.
///
/// `Ecosystem` cannot answer "could this rule have fired here". Two npm
/// formats disagree about install scripts — `package-lock.json` records
/// `hasInstallScript`, pnpm 9 dropped the field — and three PyPI formats
/// disagree about pinning, since `requirements.txt` carries a specifier while
/// `poetry.lock` and `uv.lock` carry a resolution. A rule that stays silent
/// for the second reason has found nothing; a rule that stays silent for the
/// first has been handed a file that cannot answer it. `rules::Rule::applies_to`
/// is what tells those apart, and this is what it asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Npm,
    Pnpm,
    Cargo,
    Pip,
    Poetry,
    Uv,
    GoMod,
    Yarn,
}

impl Format {
    /// The reader a filename selects, or `None` for a name no reader claims.
    ///
    /// Suffix rather than equality, so a file kept as
    /// `npm-xl.package-lock.json` still reads — lockfiles get renamed the
    /// moment you collect more than one. That only stays unambiguous while no
    /// known name is a suffix of another. None of these seven is, so the arm
    /// order is documentation rather than precedence; check it before adding
    /// an eighth.
    ///
    /// The suffixes are `KNOWN` a second time, which is a duplication a test
    /// keeps honest rather than machinery: deriving one from the other in a
    /// const context costs more than `every_known_name_has_a_format` does.
    pub fn of(name: &str) -> Option<Format> {
        let table = [
            ("package-lock.json", Format::Npm),
            ("pnpm-lock.yaml", Format::Pnpm),
            ("Cargo.lock", Format::Cargo),
            ("requirements.txt", Format::Pip),
            ("poetry.lock", Format::Poetry),
            ("uv.lock", Format::Uv),
            ("go.mod", Format::GoMod),
            ("yarn.lock", Format::Yarn),
        ];
        table
            .into_iter()
            .find(|(suffix, _)| name.ends_with(suffix))
            .map(|(_, format)| format)
    }
}

/// Read one lockfile, dispatching on its name.
pub fn read(path: &std::path::Path) -> crate::error::Result<Tree> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| crate::error::Error::io(path.display().to_string(), e))?;
    // Decided, not defaulted. This was `unwrap_or_default()`, which turned a
    // filename that is not UTF-8 into the empty string and then rendered
    // `: not a lockfile stranger knows` — an error message with nothing before
    // the colon, about a file whose name the reader never got told. The walk
    // cannot produce such a path (it matches on `to_str`), but `read` is public
    // and a caller can hand it anything.
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Err(crate::error::Error::usage(format!(
            "{}: filename is not valid UTF-8, so stranger cannot tell which format it is",
            path.display()
        )));
    };
    let tree = match Format::of(name) {
        Some(Format::Npm) => npm::read(path, &src),
        Some(Format::Pnpm) => pnpm::read(path, &src),
        Some(Format::Cargo) => cargo::read(path, &src),
        Some(Format::Pip) => pip::read(path, &src),
        Some(Format::Poetry) => pypi::poetry(path, &src),
        Some(Format::Uv) => pypi::uv(path, &src),
        Some(Format::GoMod) => gomod::read(path, &src),
        Some(Format::Yarn) => yarn::read(path, &src),
        None => Err(crate::error::Error::usage(format!(
            "{name}: not a lockfile stranger knows. It reads: {}",
            KNOWN.join(", ")
        ))),
    };
    // The syntax errors arrive from a parser that was handed a string and
    // never learned where it came from, so this is the first frame that can
    // say which file `1:1` is in. One call here covers all seven readers.
    tree.map(scrub).map_err(|e| e.in_file(path))
}

/// Replace control characters in everything a reader took from the file.
///
/// Here rather than in the renderer, and that is the point: `report.rs`,
/// `tree.rs` and every rule's `detail` string all print package names, so
/// "sanitise at the print site" is a rule with a dozen call sites and one of
/// them will be missed by whoever adds the thirteenth. This is the one seam
/// every reader already passes through, so a name that reaches the rest of the
/// program is a name that has been through it. See `term::sanitize` for what a
/// hostile version string does to a terminal without it.
///
/// The JSON writer escapes correctly and never needed this, but it gets the
/// scrubbed string too — the two surfaces disagreeing about what a package is
/// called would be worse than either answer.
fn scrub(mut tree: Tree) -> Tree {
    fn clean(s: &mut String) {
        if let std::borrow::Cow::Owned(safe) = crate::term::sanitize(s) {
            *s = safe;
        }
    }
    for pkg in &mut tree.packages {
        clean(&mut pkg.name);
        clean(&mut pkg.version);
        clean(&mut pkg.key);
        if let Pin::Compatible(spec) | Pin::Range(spec) = &mut pkg.pinned {
            clean(spec);
        }
    }
    tree
}

/// Every known lockfile under `dir`, and everywhere the walk could not look.
///
/// The walk is what makes this usable on a monorepo, and the skip list in
/// `crate::walk` is what keeps it from wandering into `node_modules` and
/// auditing four hundred vendored lockfiles belonging to other people. What it
/// skipped and what it could not open come back too — see `walk::Walk`.
pub fn discover(dir: &std::path::Path) -> crate::walk::Walk {
    crate::walk::lockfiles(dir, KNOWN)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two lists of suffixes have to name the same eight files. `KNOWN` is
    /// what the walk matches on and what "looked for:" prints; `Format::of` is
    /// what picks a reader. A name in one and not the other is either a
    /// lockfile found and then refused, or one advertised and never seen.
    #[test]
    fn every_known_name_has_a_format() {
        let mut formats: Vec<Format> = KNOWN
            .iter()
            .map(|k| Format::of(k).unwrap_or_else(|| panic!("{k} has no reader")))
            .collect();
        let seen = formats.len();
        formats.dedup();
        assert_eq!(formats.len(), seen, "two names picked the same reader");
    }

    /// Fixtures are kept as `npm-xl.package-lock.json`, so the match is on the
    /// end of the name — and a name nothing claims stays unclaimed rather than
    /// falling through to whichever arm is last.
    #[test]
    fn a_prefixed_fixture_still_picks_its_reader() {
        assert_eq!(Format::of("npm-xl.package-lock.json"), Some(Format::Npm));
        assert_eq!(Format::of("reqs-s.requirements.txt"), Some(Format::Pip));
        assert_eq!(Format::of("yarn-l.yarn.lock"), Some(Format::Yarn));
        assert_eq!(Format::of("packages.lock.json"), None);
        assert_eq!(Format::of("bun.lockb"), None);
    }
}
