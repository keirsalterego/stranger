use stranger::semver::{Req, Version};

fn v(s: &str) -> Version {
    Version::parse(s).unwrap_or_else(|| panic!("{s:?} should parse"))
}

#[test]
fn parses() {
    assert_eq!(
        v("1.2.3"),
        Version {
            major: 1,
            minor: 2,
            patch: 3,
            pre: vec![]
        }
    );
    assert_eq!(v("v1.2.3"), v("1.2.3"));
    assert_eq!(v("1"), v("1.0.0"));
    assert_eq!(v("1.2"), v("1.2.0"));
    assert_eq!(v("1.0.0-alpha.1").pre, vec!["alpha", "1"]);
}

#[test]
fn rejects_junk() {
    for s in ["", "abc", "1.2.3.4", "1.-2.3", "1.x.3", "-1.0.0"] {
        assert!(Version::parse(s).is_none(), "{s:?} should not parse");
    }
}

/// Build metadata is ignored for precedence, so it is dropped at parse time.
#[test]
fn build_metadata_is_dropped() {
    assert_eq!(v("1.0.0+sha.5114f85"), v("1.0.0"));
    assert_eq!(v("1.0.0-rc.1+build.1"), v("1.0.0-rc.1"));
}

/// The exact ordering table from semver.org section 11. If an implementation
/// is going to be wrong, it is wrong somewhere in this line.
#[test]
fn the_spec_ordering_table() {
    let ordered = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
    ];
    for pair in ordered.windows(2) {
        assert!(
            v(pair[0]) < v(pair[1]),
            "{} should sort below {}",
            pair[0],
            pair[1]
        );
    }
    // And the whole line is consistent under sorting, not just pairwise.
    let mut shuffled: Vec<Version> = ordered.iter().rev().map(|s| v(s)).collect();
    shuffled.sort();
    let expected: Vec<Version> = ordered.iter().map(|s| v(s)).collect();
    assert_eq!(shuffled, expected);
}

/// `1.0.0-beta.11 > 1.0.0-beta.2` only if numeric segments compare as numbers.
/// String comparison puts "11" before "2" and gets this backwards.
#[test]
fn numeric_segments_are_not_strings() {
    assert!(v("1.0.0-beta.11") > v("1.0.0-beta.2"));
    assert!(
        v("1.0.0-2") < v("1.0.0-11"),
        "as strings, \"11\" sorts before \"2\""
    );
    // Numeric sorts below alphanumeric.
    assert!(v("1.0.0-1") < v("1.0.0-alpha"));
}

#[test]
fn a_release_outranks_its_prereleases() {
    assert!(v("1.0.0") > v("1.0.0-rc.1"));
    assert!(v("1.0.0") > v("1.0.0-alpha"));
    assert!(v("2.0.0-alpha") > v("1.99.99"));
}

#[test]
fn core_ordering() {
    assert!(
        v("1.9.0") < v("1.10.0"),
        "1.10.0 sorts above 1.9.0, unlike as strings"
    );
    assert!(v("1.0.0") < v("2.0.0"));
    assert!(v("1.0.1") > v("1.0.0"));
}

#[test]
fn caret_holds_the_leftmost_nonzero() {
    let r = Req::parse("^1.2.3").unwrap();
    assert!(r.matches(&v("1.2.3")));
    assert!(r.matches(&v("1.9.9")));
    assert!(!r.matches(&v("2.0.0")));
    assert!(!r.matches(&v("1.2.2")));

    let r = Req::parse("^0.2.3").unwrap();
    assert!(r.matches(&v("0.2.9")));
    assert!(
        !r.matches(&v("0.3.0")),
        "under 1.0.0 the minor acts as the major"
    );

    let r = Req::parse("^0.0.3").unwrap();
    assert!(r.matches(&v("0.0.3")));
    assert!(!r.matches(&v("0.0.4")));
}

#[test]
fn tilde_moves_the_patch() {
    let r = Req::parse("~1.2.3").unwrap();
    assert!(r.matches(&v("1.2.9")));
    assert!(!r.matches(&v("1.3.0")));
}

#[test]
fn comparison_operators() {
    assert!(Req::parse(">=1.0").unwrap().matches(&v("1.0.0")));
    assert!(Req::parse(">=1.0").unwrap().matches(&v("9.9.9")));
    assert!(!Req::parse(">1.0.0").unwrap().matches(&v("1.0.0")));
    assert!(Req::parse("<2").unwrap().matches(&v("1.9.9")));
    assert!(Req::parse("==2.31.0").unwrap().matches(&v("2.31.0")));
}

/// `>=` must not parse as `>`.
#[test]
fn longest_operator_wins() {
    assert_eq!(Req::parse(">=1.0.0"), Some(Req::GreaterEq(v("1.0.0"))));
    assert_eq!(Req::parse("<=1.0.0"), Some(Req::LessEq(v("1.0.0"))));
    assert_eq!(Req::parse("==1.0.0"), Some(Req::Exact(v("1.0.0"))));
}

#[test]
fn anything_matches_any() {
    for s in ["*", "", "latest", "x"] {
        assert_eq!(Req::parse(s), Some(Req::Any), "{s:?}");
        assert!(Req::parse(s).unwrap().matches(&v("0.0.1")));
    }
}

#[test]
fn displays_back() {
    for s in ["1.2.3", "1.0.0-alpha.1", "0.0.1"] {
        assert_eq!(v(s).to_string(), s);
    }
}
