use stranger::corpus;
use stranger::lock::Ecosystem;

const ALL: &[Ecosystem] = &[Ecosystem::Npm, Ecosystem::PyPi, Ecosystem::Crates];

/// `binary_search` on an unsorted slice does not fail loudly, it just returns
/// the wrong answer for some inputs. Shell `sort` is locale-dependent, so the
/// files are generated with `LC_ALL=C` and checked here rather than trusted.
#[test]
fn every_corpus_is_sorted_in_byte_order() {
    for &eco in ALL {
        let names = corpus::names(eco);
        assert!(!names.is_empty(), "{} corpus is empty", eco.as_str());
        assert!(
            names.windows(2).all(|w| w[0] < w[1]),
            "{} corpus is not sorted and deduplicated",
            eco.as_str()
        );
    }
}

#[test]
fn names_are_already_normalised() {
    for &eco in ALL {
        for name in corpus::names(eco) {
            assert_eq!(
                &corpus::normalize(eco, name),
                name,
                "{} holds {name:?} unnormalised",
                eco.as_str()
            );
        }
    }
}

#[test]
fn sizes() {
    assert_eq!(corpus::names(Ecosystem::Crates).len(), 5_000);
    assert_eq!(corpus::names(Ecosystem::PyPi).len(), 15_000);
    assert_eq!(corpus::names(Ecosystem::Npm).len(), 140_066);
    // No ranked list of Go module paths exists, so the rule never fires there.
    // Empty on purpose, and said out loud in README LIMITS.
    assert!(corpus::names(Ecosystem::Go).is_empty());
}

/// The two-letter registry sweep missed `lodash`, which would have made the
/// tool report one of the most depended-upon packages on npm as a
/// hallucination. These are the names that gap was caught with.
#[test]
fn popular_packages_are_present() {
    for name in [
        "lodash",
        "chalk",
        "express",
        "react",
        "typescript",
        "webpack",
        "eslint",
    ] {
        assert!(
            corpus::contains(Ecosystem::Npm, name),
            "npm is missing {name}"
        );
    }
    for name in ["requests", "python-dateutil", "numpy", "flask", "urllib3"] {
        assert!(
            corpus::contains(Ecosystem::PyPi, name),
            "pypi is missing {name}"
        );
    }
    for name in ["serde_json", "clap", "toml", "walkdir", "strsim"] {
        assert!(
            corpus::contains(Ecosystem::Crates, name),
            "crates.io is missing {name}"
        );
    }
}

#[test]
fn planted_names_are_absent() {
    for name in ["expres", "lodahs", "chalck"] {
        assert!(
            !corpus::contains(Ecosystem::Npm, name),
            "npm unexpectedly has {name}"
        );
    }
    for name in ["python-dateutils", "requests-http"] {
        assert!(
            !corpus::contains(Ecosystem::PyPi, name),
            "pypi unexpectedly has {name}"
        );
    }
}

/// PEP 503: PyPI treats these as one project, so a separator choice must not
/// read as a typo.
#[test]
fn pypi_separators_are_equivalent() {
    for spelling in [
        "python-dateutil",
        "python_dateutil",
        "Python.DateUtil",
        "python--dateutil",
    ] {
        assert!(corpus::contains(Ecosystem::PyPi, spelling), "{spelling}");
    }
}

#[test]
fn nearest_finds_the_obvious_parent() {
    assert_eq!(
        corpus::nearest(Ecosystem::Npm, "expres"),
        Some(("express", 1))
    );
    assert_eq!(
        corpus::nearest(Ecosystem::Npm, "lodahs"),
        Some(("lodash", 1))
    );
    assert_eq!(
        corpus::nearest(Ecosystem::Npm, "chalck"),
        Some(("chalk", 1))
    );
    assert_eq!(
        corpus::nearest(Ecosystem::PyPi, "python-dateutils"),
        Some(("python-dateutil", 1))
    );
}

/// A name that is not a near-miss of anything real gets no neighbour, and the
/// rule stays quiet rather than inventing one.
#[test]
fn nearest_gives_up_on_names_that_are_not_typos() {
    assert_eq!(
        corpus::nearest(Ecosystem::Npm, "zzqxwvunexistentpackage"),
        None
    );
    assert_eq!(
        corpus::nearest(Ecosystem::Go, "github.com/whatever/thing"),
        None
    );
}
