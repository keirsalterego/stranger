//! Semantic versions: parsing, precedence, and the two range operators that
//! actually appear in lockfiles.
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

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
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

/// The constraint operators that turn up in a lockfile or a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Req {
    Exact(Version),
    /// `^1.2.3` — the leftmost non-zero component may not change. npm and
    /// Cargo agree on this, including the awkward `^0.x` and `^0.0.x` cases.
    Caret(Version),
    /// `~1.2.3` — patch may move. `~1.2` allows minor.
    Tilde(Version),
    GreaterEq(Version),
    Greater(Version),
    LessEq(Version),
    Less(Version),
    /// `*`, `latest`, an empty string: anything at all.
    Any,
}

impl Req {
    pub fn parse(s: &str) -> Option<Req> {
        let s = s.trim();
        if s.is_empty() || s == "*" || s == "latest" || s == "x" {
            return Some(Req::Any);
        }
        // Longest operators first, or `>=` parses as `>`.
        for (prefix, make) in [
            (">=", Req::GreaterEq as fn(Version) -> Req),
            ("<=", Req::LessEq),
            ("==", Req::Exact),
            ("^", Req::Caret),
            ("~", Req::Tilde),
            (">", Req::Greater),
            ("<", Req::Less),
            ("=", Req::Exact),
        ] {
            if let Some(rest) = s.strip_prefix(prefix) {
                return Version::parse(rest).map(make);
            }
        }
        Version::parse(s).map(Req::Exact)
    }

    pub fn matches(&self, v: &Version) -> bool {
        match self {
            Req::Any => true,
            Req::Exact(r) => v == r,
            Req::GreaterEq(r) => v >= r,
            Req::Greater(r) => v > r,
            Req::LessEq(r) => v <= r,
            Req::Less(r) => v < r,
            Req::Caret(r) => v >= r && caret_upper(r).is_none_or(|hi| *v < hi),
            Req::Tilde(r) => v >= r && *v < tilde_upper(r),
        }
    }
}

/// `^1.2.3` -> `<2.0.0`, `^0.2.3` -> `<0.3.0`, `^0.0.3` -> `<0.0.4`.
///
/// The zero cases are not a special-case hack, they fall straight out of "the
/// leftmost non-zero component may not change" — under 1.0.0 the minor is
/// effectively the major.
fn caret_upper(r: &Version) -> Option<Version> {
    let bump = if r.major > 0 {
        Version {
            major: r.major + 1,
            minor: 0,
            patch: 0,
            pre: Vec::new(),
        }
    } else if r.minor > 0 {
        Version {
            major: 0,
            minor: r.minor + 1,
            patch: 0,
            pre: Vec::new(),
        }
    } else {
        Version {
            major: 0,
            minor: 0,
            patch: r.patch + 1,
            pre: Vec::new(),
        }
    };
    Some(bump)
}

fn tilde_upper(r: &Version) -> Version {
    Version {
        major: r.major,
        minor: r.minor + 1,
        patch: 0,
        pre: Vec::new(),
    }
}
