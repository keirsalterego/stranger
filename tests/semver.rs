use stranger::semver::Version;

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
fn displays_back() {
    for s in ["1.2.3", "1.0.0-alpha.1", "0.0.1"] {
        assert_eq!(v(s).to_string(), s);
    }
}

/// The reason `Version` is still here at all.
///
/// `drift` prints one line per duplicated name listing every version of it,
/// and it used to sort those by byte order — which puts `10.1.0` before
/// `7.0.1`, because `1` precedes `7` one character at a time. 29 of the 448
/// drift findings across the fixtures came out misordered, three of them on
/// the poisoned fixture the README demos.
#[test]
fn drift_prints_versions_in_precedence_order() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("npm-xl.package-lock.json");
    let text = std::fs::read_to_string(&path).expect("npm-xl fixture");
    let tree = stranger::lock::npm::read(&path, &text).expect("npm-xl parses");

    let findings = stranger::rules::drift::scan(&tree);
    assert!(
        findings.len() > 50,
        "expected the real fixture, got {}",
        findings.len()
    );

    let mut checked = 0;
    for f in &findings {
        let listed: Vec<Version> = f
            .detail
            .split(": ")
            .nth(1)
            .expect("detail names its versions")
            .split(", ")
            .map(|s| Version::parse(s).unwrap_or_else(|| panic!("{s:?} in {}", f.package)))
            .collect();
        let mut sorted = listed.clone();
        sorted.sort();
        assert_eq!(listed, sorted, "{} lists versions out of order", f.package);
        checked += listed.len();
    }
    assert_eq!(
        checked, 180,
        "npm-xl holds 76 drifted names across 180 versions"
    );
}

/// The case that byte order gets wrong, in isolation, so a failure above is
/// easy to read.
#[test]
fn ten_sorts_after_nine() {
    let mut vs = ["10.1.0", "7.0.1", "9.1.0", "11.3.5", "8.1.0", "11.3.1"];
    vs.sort_by_key(|s| Version::parse(s).expect("parses"));
    assert_eq!(
        vs,
        ["7.0.1", "8.1.0", "9.1.0", "10.1.0", "11.3.1", "11.3.5"]
    );

    let mut bytes = vs;
    bytes.sort_unstable();
    assert_ne!(bytes, vs, "if these agree the test proves nothing");
}
