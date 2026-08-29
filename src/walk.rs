//! Finding lockfiles under a directory.
//!
//! Three things make this more than a `read_dir` loop.
//!
//! **`node_modules` must not be descended into.** A populated `node_modules`
//! contains hundreds of vendored `package-lock.json` files belonging to other
//! people's projects. Walking into it turns one scan into four hundred, all of
//! them irrelevant, and is the difference between a tool that works on a real
//! checkout and one that only works on the fixtures.
//!
//! **Order has to be deterministic.** `read_dir` returns entries in whatever
//! order the filesystem feels like, which on ext4 is hash order. Two runs on
//! the same tree would report the same findings in a different sequence, and a
//! diff between two scans would be mostly noise. Entries are sorted.
//!
//! **A walk that could not see everything has to say so.** Not descending into
//! `node_modules` is a decision; failing to open a directory is not, and a
//! `Vec<PathBuf>` spells both of them as a shorter list. `stranger` must never
//! be unable to tell "I looked and found nothing" from "I could not look", so
//! this hands back what it passed over alongside what it found — see `Walk`.
//!
//! Symlinked *directories* are not followed. A symlink to a parent directory
//! is an infinite walk, and `..`-shaped symlinks exist in real `node_modules`
//! trees. Symlinks to files are followed: a monorepo that keeps one lockfile
//! and links it from every package is doing a normal thing, and refusing to
//! read it means silently reporting "no lockfile" on a tree that has one.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Directories that never contain a lockfile worth auditing, only vendored
/// copies of other people's.
///
/// Thirteen names, and `dist` is the one that is a guess rather than a fact:
/// most `dist/` directories are build output, some are a package somebody
/// publishes from, and this list cannot tell them apart. Every hidden
/// directory goes too, which is a much bigger cut than thirteen names — `.ci`
/// and `.github` are places people keep a real lockfile. Both cuts are printed
/// under `-v` rather than argued about here.
const SKIP: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "dist",
    ".next",
    ".svelte-kit",
];

/// Lockfile names `stranger` recognises and has no reader for.
///
/// Naming a file it will not read is not the same as reading it, and it is
/// strictly better than the silence it replaces: eight lockfiles in a
/// directory used to print "no lockfile in .", which tells somebody with a
/// yarn project that their repository has no lockfile. Not reading `yarn.lock`
/// is a declared cut in DECISIONS.md, and a cut the user can see is a
/// different thing from one only the author knows about.
///
/// Matched exactly, unlike `lock::KNOWN`, which matches on the end of the name
/// because this repository keeps fixtures called `npm-xl.package-lock.json`.
/// Nobody keeps an `old.yarn.lock` and needs to be told about it.
const UNSUPPORTED: &[&str] = &[
    "Gemfile.lock",
    "Pipfile.lock",
    "Podfile.lock",
    "bun.lock",
    "bun.lockb",
    "composer.lock",
    "conan.lock",
    "go.sum",
    "gradle.lockfile",
    "mix.lock",
    "packages.lock.json",
    "pdm.lock",
    "pubspec.lock",
    "yarn.lock",
];

/// How deep to go. A monorepo keeps its manifests within two or three levels of
/// the root; past that you are almost certainly inside something vendored that
/// the skip list did not name.
///
/// Counted below the directory you named, so a lockfile six levels down is
/// found and one seven levels down is not. It is a guess about other people's
/// layouts rather than a fact about them, which is why every directory it cuts
/// off is printed under `-v` instead of vanishing.
pub const MAX_DEPTH: usize = 6;

/// What one walk saw, including what it did not.
///
/// The three lists past `found` are each a silent zero that used to reach the
/// caller as "no lockfile here": a directory that would not open, a lockfile in
/// a format with no reader, and a subtree below `MAX_DEPTH` were all
/// indistinguishable from an empty directory and all routed to the same clean
/// exit 0.
#[derive(Debug, Default)]
pub struct Walk {
    /// Lockfiles with a reader behind them, sorted.
    pub found: Vec<PathBuf>,
    /// Paths the walk could not inspect: a directory `read_dir` refused, or an
    /// entry it could not stat. Sorted. Not the same as absent — a path that is
    /// not there hides nothing and does not appear here.
    pub unreadable: Vec<PathBuf>,
    /// Lockfile names seen with no reader behind them, deduped and sorted. A
    /// monorepo with ten `yarn.lock` files says `yarn.lock` once.
    pub unsupported: Vec<&'static str>,
    /// Directories not entered on purpose, each with the reason. Policy rather
    /// than failure, so only `-v` prints them — but policy that hides a
    /// lockfile hides a lockfile.
    pub skipped: Vec<(PathBuf, &'static str)>,
}

/// Every lockfile `stranger` knows how to read under `root`, and everything it
/// passed over on the way.
///
/// ponytail: returns a `Walk` of `Vec`s rather than an iterator. The result is
/// a handful of paths that get iterated once, and a lazy walker would mean
/// threading the stack and the depth through a struct to save allocating six
/// `PathBuf`s.
pub fn lockfiles(root: &Path, known: &[&str]) -> Walk {
    let mut found = BTreeSet::new();
    let mut unsupported = BTreeSet::new();
    let mut unreadable = Vec::new();
    let mut skipped = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            // A directory that is not there hides nothing, so it is not a
            // blind spot. One that is there and will not open takes an unknown
            // number of lockfiles out of the answer, and that distinction is
            // the whole reason this list exists — the caller turns it into an
            // exit code.
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(_) => {
                unreadable.push(dir);
                continue;
            }
        };

        let mut children = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                // Named in the listing and not stattable: the same blind spot
                // as an unopenable directory, one entry wide.
                unreadable.push(path);
                continue;
            };

            // `file_type` does not follow the link, so a symlink is neither a
            // file nor a dir here and has to be resolved by hand. Only the
            // directory case is dangerous — that is the cycle.
            let kind = if kind.is_symlink() {
                match std::fs::metadata(&path) {
                    // Nothing on the end of it, so nothing was missed.
                    Err(e) if e.kind() == ErrorKind::NotFound => continue,
                    Err(_) => {
                        unreadable.push(path);
                        continue;
                    }
                    Ok(target) if target.is_dir() => {
                        skipped.push((path, "symlinked directory"));
                        continue;
                    }
                    Ok(target) => target.file_type(),
                }
            } else {
                kind
            };

            if kind.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                // Name before depth, so a `node_modules` seven levels down
                // reports as vendored rather than as a depth casualty. Where
                // two reasons apply the first one is the one worth printing.
                if SKIP.contains(&name.as_ref()) {
                    skipped.push((path, "on the skip list"));
                } else if name.starts_with('.') && name.len() > 1 {
                    skipped.push((path, "hidden"));
                } else if depth + 1 > MAX_DEPTH {
                    skipped.push((path, "below MAX_DEPTH"));
                } else {
                    children.push(path);
                }
            } else if kind.is_file()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                if known.iter().any(|k| name.ends_with(k)) {
                    found.insert(path);
                } else if let Some(&no_reader) = UNSUPPORTED.iter().find(|u| **u == name) {
                    unsupported.insert(no_reader);
                }
            }
        }

        // Pushed in reverse so the pop order matches the sorted order. The
        // BTreeSet makes the final result sorted regardless; this just keeps
        // the traversal itself predictable when debugging.
        children.sort();
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
    }

    // Sorted for the reason `found` is: these get printed, and two runs over
    // one tree have to produce the same bytes.
    unreadable.sort();
    skipped.sort();

    Walk {
        found: found.into_iter().collect(),
        unreadable,
        unsupported: unsupported.into_iter().collect(),
        skipped,
    }
}
