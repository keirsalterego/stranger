//! Semantic versions: parsing and precedence.
//!
//! There was a range matcher here too — caret, tilde, the comparison
//! operators — written on the assumption something would need to ask whether
//! a version satisfied a constraint. Nothing ever did. Every reader resolves
//! edges by name and path, because that is what a *lock*file records: the
//! answer, not the question. It is deleted rather than kept as decoration,
//! and it had two real bugs in it when I went to check, which is the usual
//! fate of code nothing calls.
//!
//! What is left is used: `drift` sorts the versions it prints with `Ord`.
//!
//! The part worth getting right is precedence, and specifically prerelease
//! precedence, which semver.org spells out in section 11 and which almost
//! everybody implements by accident. Three rules do the damage:
//!
//! - a version *with* a prerelease has **lower** precedence than the same
//!   version without one, so `1.0.0-rc.1 < 1.0.0`
//! - identifiers are compared dot-segment by dot-segment; a segment of digits
//!   compares numerically, anything else compares as ASCII, and **numeric
//!   sorts below alphanumeric**
//! - if every shared segment is equal, more segments wins, so
//!   `1.0.0-alpha < 1.0.0-alpha.1`
//!
//! Build metadata (`+sha.5114f85`) is ignored entirely for precedence. Two
//! versions differing only in build metadata have *equal* precedence, which is
//! why `Version` implements `Ord` but not `Eq` in terms of the raw text.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Dot-separated identifiers, without the leading `-`. Empty means this is
    /// a release.
    pub pre: Vec<String>,
}

impl Version {
    /// Lenient on the shapes lockfiles actually contain: a missing minor or
    /// patch reads as zero, so `1` and `1.2` parse. Strict semver requires all
    /// three, but `requirements.txt` is full of `flask~=3.0` and refusing it
    /// would mean refusing the file.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix('v').unwrap_or(s);

        // Build metadata plays no part in precedence, so it is dropped here
        // rather than stored and then carefully ignored everywhere else.
        let s = s.split('+').next()?;

        let (core, pre) = match s.split_once('-') {
            Some((core, pre)) => (core, pre.split('.').map(str::to_owned).collect()),
            None => (s, Vec::new()),
        };

        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
        let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
        if parts.next().is_some() {
            return None;
        }

        Some(Version {
            major,
            minor,
            patch,
            pre,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| compare_pre(&self.pre, &other.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

/// semver.org section 11.4. An empty prerelease list means a release, which
/// outranks every prerelease of the same core version — the one rule that
/// inverts the usual "empty is smallest" intuition, and the one everybody
/// gets wrong.
fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (x, y) in a.iter().zip(b) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            // "Numeric identifiers always have lower precedence than
            // non-numeric identifiers." So `1.0.0-1 < 1.0.0-alpha`.
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }

    a.len().cmp(&b.len())
}
