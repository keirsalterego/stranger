//! Every JSON fixture has to parse. This is the test that would have caught
//! the escape and surrogate bugs if the unit tests had missed them, because
//! these are files nobody wrote for our convenience.

use std::fs;
use std::path::Path;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Counts are `packages` entries minus the root, measured with `jq`, not
/// copied from anyone's notes — the notes said npm-xl held 1,391 and it holds
/// 1,390. Some of these entries are `link: true` workspace members, which are
/// first-party and get excluded later; that happens in the reader, not here.
const NPM_FIXTURES: &[(&str, usize)] = &[
    ("npm-xs.package-lock.json", 37),
    ("npm-s.package-lock.json", 405),
    ("npm-m.package-lock.json", 582),
    ("npm-l.package-lock.json", 754),
    ("npm-xl.package-lock.json", 1390),
    ("poisoned.package-lock.json", 757),
];

#[test]
fn every_npm_fixture_parses() {
    for (name, expected) in NPM_FIXTURES {
        let src = fixture(name);
        let v = stranger::json::parse(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let packages = v
            .get("packages")
            .and_then(|p| p.as_object())
            .unwrap_or_else(|| panic!("{name}: no packages map"));
        // The root project is the empty-string key, so it is in the map but is
        // not one of the dependencies being counted.
        assert_eq!(packages.len() - 1, *expected, "{name}");
    }
}

#[test]
fn lockfile_version_is_three_everywhere() {
    for (name, _) in NPM_FIXTURES {
        let src = fixture(name);
        let v = stranger::json::parse(&src).unwrap();
        assert_eq!(
            v.get("lockfileVersion"),
            Some(&stranger::json::Value::Number(3.0)),
            "{name}"
        );
    }
}
