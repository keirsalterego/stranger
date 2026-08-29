//! Finding lockfiles under a directory.
//!
//! Two things make this more than a `read_dir` loop.
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
//! Symlinked *directories* are not followed. A symlink to a parent directory
//! is an infinite walk, and `..`-shaped symlinks exist in real `node_modules`
//! trees. Symlinks to files are followed: a monorepo that keeps one lockfile
//! and links it from every package is doing a normal thing, and refusing to
//! read it means silently reporting "no lockfile" on a tree that has one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Directories that never contain a lockfile worth auditing, only vendored
/// copies of other people's.
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

/// How deep to go. A monorepo keeps its manifests within two or three levels of
/// the root; past that you are almost certainly inside something vendored that
/// the skip list did not name.
pub const MAX_DEPTH: usize = 6;

/// Every lockfile `stranger` knows how to read, under `root`, sorted.
///
/// ponytail: returns a `Vec` rather than an iterator. The result is a handful
/// of paths that get iterated once, and a lazy walker would mean threading the
/// stack and the depth through a struct to save allocating six `PathBuf`s.
pub fn lockfiles(root: &Path, known: &[&str]) -> Vec<PathBuf> {
    let mut found = BTreeSet::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            // An unreadable directory is not an error worth stopping a scan
            // for — a permissions-denied `.cache` in someone's home directory
            // should not take down an audit of the project beside it.
            continue;
        };

        let mut children = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };

            // `file_type` does not follow the link, so a symlink is neither a
            // file nor a dir here and has to be resolved by hand. Only the
            // directory case is dangerous — that is the cycle — and a dangling
            // link resolves to nothing, so both fall out as `continue`.
            let kind = if kind.is_symlink() {
                let Ok(target) = std::fs::metadata(&path) else {
                    continue;
                };
                if target.is_dir() {
                    continue;
                }
                target.file_type()
            } else {
                kind
            };

            if kind.is_dir() {
                if depth + 1 > MAX_DEPTH {
                    continue;
                }
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') && name.len() > 1 || SKIP.contains(&name.as_ref()) {
                    continue;
                }
                children.push(path);
            } else if kind.is_file()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && known.iter().any(|k| name.ends_with(k))
            {
                found.insert(path);
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

    found.into_iter().collect()
}
