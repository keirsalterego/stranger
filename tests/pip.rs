use std::fs;
use std::path::{Path, PathBuf};
use stranger::lock::{Ecosystem, Origin, Pin, Tree, pip};
use stranger::rules::{Rule, Severity, pinning, slopsquat};

fn load(name: &str) -> Tree {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let src = fs::read_to_string(&path).unwrap();
    pip::read(&path, &src).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn parse(src: &str) -> Tree {
    pip::read(Path::new("requirements.txt"), src).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn pin_of(t: &Tree, name: &str) -> Pin {
    t.packages
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
        .pinned
        .clone()
}

/// Counted with `awk 'NF && $0 !~ /^[ \t]*#/'` against the files in
/// `fixtures/`, not copied from anywhere. `fixtures/README.md` lists
/// `reqs-xs` as 11 and it holds 12; the file is the authority.
#[test]
fn counts() {
    assert_eq!(load("reqs-xs.requirements.txt").packages.len(), 12);
    assert_eq!(load("reqs-s.requirements.txt").packages.len(), 23);
    assert_eq!(load("poisoned.requirements.txt").packages.len(), 6);
}

/// The format has no graph in it. Asserted rather than assumed, because the
/// slopsquat rule reads `in_degree` and would otherwise be leaning on a fact
/// nobody had checked.
#[test]
fn flat() {
    for name in [
        "reqs-xs.requirements.txt",
        "reqs-s.requirements.txt",
        "poisoned.requirements.txt",
    ] {
        let t = load(name);
        assert_eq!(t.ecosystem, Ecosystem::PyPi);
        assert!(t.edges.is_empty(), "{name} grew an edge");
        assert_eq!(t.roots.len(), t.packages.len(), "{name}");
        assert_eq!(t.transitive(), 0, "{name}");
        assert!(t.in_degree().iter().all(|&d| d == 0), "{name}");
    }
}

/// `Pillow` is in `reqs-s` with a capital P and PyPI serves it as `pillow`.
/// The reader keeps the file's spelling; `corpus::normalize` closes the gap.
#[test]
fn names_kept_as_written() {
    let t = load("reqs-s.requirements.txt");
    assert!(t.packages.iter().any(|p| p.name == "Pillow"));
    assert!(stranger::corpus::contains(Ecosystem::PyPi, "Pillow"));
    assert!(t.packages.iter().any(|p| p.name == "python-dateutil"));
}

#[test]
fn exact_pins() {
    let t = parse("requests==2.31.0\nsix===1.16.0\n");
    assert_eq!(pin_of(&t, "requests"), Pin::Exact);
    assert_eq!(t.packages[0].version, "2.31.0");
    assert_eq!(pin_of(&t, "six"), Pin::Exact);
    assert_eq!(t.packages[1].version, "1.16.0");
}

#[test]
fn constraint_kinds() {
    let t = parse("a>=1.0\nb~=1.2\nc<2\nd!=1.5\ne\nf==1.2.*\ng>=1.0,<2\nh>=1.0,==1.4\n");
    assert_eq!(pin_of(&t, "a"), Pin::Range(">=1.0".into()));
    assert_eq!(pin_of(&t, "b"), Pin::Compatible("~=1.2".into()));
    assert_eq!(pin_of(&t, "c"), Pin::Range("<2".into()));
    assert_eq!(pin_of(&t, "d"), Pin::Range("!=1.5".into()));
    assert_eq!(pin_of(&t, "e"), Pin::Unconstrained);
    // `==1.2.*` wears the exact operator and is a range of releases.
    assert_eq!(pin_of(&t, "f"), Pin::Compatible("==1.2.*".into()));
    assert_eq!(pin_of(&t, "g"), Pin::Range(">=1.0,<2".into()));
    // The tightest clause wins: a `==` anywhere means one version installs.
    assert_eq!(pin_of(&t, "h"), Pin::Exact);
    // Only an exact pin gets to claim a version.
    for p in &t.packages {
        assert_eq!(p.version.is_empty(), p.pinned != Pin::Exact, "{}", p.name);
    }
}

#[test]
fn extras_do_not_eat_the_specifier() {
    let t = parse("flask[async,dotenv]==3.0.0\nuvicorn[standard]\n");
    assert_eq!(t.packages[0].name, "flask");
    assert_eq!(t.packages[0].version, "3.0.0");
    assert_eq!(pin_of(&t, "uvicorn"), Pin::Unconstrained);
}

/// A marker carries `<` and `==` of its own. Read in the wrong order this
/// reports a pinned requirement as a range, which is a false finding rather
/// than a parse failure — so it gets its own test.
#[test]
fn markers_are_not_specifiers() {
    let t = parse(
        "importlib-metadata==6.0.0; python_version < \"3.10\"\n\
         tomli; python_version<\"3.11\"\n\
         backports[x]>=1.0 ; sys_platform == \"win32\"\n",
    );
    assert_eq!(pin_of(&t, "importlib-metadata"), Pin::Exact);
    assert_eq!(t.packages[0].version, "6.0.0");
    assert_eq!(pin_of(&t, "tomli"), Pin::Unconstrained);
    assert_eq!(pin_of(&t, "backports"), Pin::Range(">=1.0".into()));
}

#[test]
fn comments_and_blanks() {
    let t = parse("# header\n\n   \nrequests==2.31.0  # why\n\t# indented\nsix==1.16.0\n");
    assert_eq!(t.packages.len(), 2);
    assert_eq!(t.packages[0].version, "2.31.0");
}

/// pip only treats `#` as a comment at line start or after whitespace, which
/// is what keeps a URL fragment intact. Here it means the `#` stays glued to
/// the version, and the version check rejects it instead of silently
/// recording `1.0#note`.
#[test]
fn hash_without_space_is_not_a_comment() {
    let err = pip::read(Path::new("r.txt"), "requests==1.0#note\n").unwrap_err();
    assert!(err.to_string().contains("at 1:1"), "{err}");
}

#[test]
fn option_lines_are_not_packages() {
    let t = parse(
        "-r base.txt\n-c constraints.txt\n-e .\n--index-url https://pypi.org/simple\n\
         --extra-index-url https://internal.example/simple\n--no-binary :all:\nsix==1.16.0\n",
    );
    assert_eq!(t.packages.len(), 1);
    assert_eq!(t.packages[0].name, "six");
}

#[test]
fn continuations_and_hashes() {
    let t = parse(
        "requests==2.31.0 \\\n    --hash=sha256:aaa \\\n    --hash=sha256:bbb\nsix==1.16.0\n",
    );
    assert_eq!(t.packages.len(), 2);
    assert_eq!(t.packages[0].name, "requests");
    assert_eq!(t.packages[0].version, "2.31.0");
    assert!(t.packages[0].has_integrity);
    assert!(!t.packages[1].has_integrity);
}

/// Hashes sit after the marker, so options have to be split off before the
/// marker is cut or the `--hash` disappears with it.
#[test]
fn hashes_survive_a_marker() {
    let t = parse("tomli==2.0.1 ; python_version<\"3.11\" --hash=sha256:aaa\n");
    assert_eq!(t.packages[0].version, "2.0.1");
    assert!(t.packages[0].has_integrity);
}

/// PEP 508 makes whitespace inside a requirement insignificant, and the
/// reader relies on that when it glues the tokens back together.
#[test]
fn loose_whitespace() {
    let t = parse("flask [async] >= 3.0\n");
    assert_eq!(t.packages[0].name, "flask");
    assert_eq!(pin_of(&t, "flask"), Pin::Range(">=3.0".into()));
}

#[test]
fn direct_url_has_no_version_to_pin() {
    let t = parse("mypkg @ https://example.invalid/mypkg-1.0.whl\n");
    assert_eq!(t.packages[0].name, "mypkg");
    assert_eq!(pin_of(&t, "mypkg"), Pin::Unconstrained);
}

/// The other half of that line, and the half that was wrong: a direct
/// reference does not come from PyPI, so the PyPI corpus has nothing to say
/// about the name.
///
/// This was read off the glued spec rather than off the text after the name,
/// and the glued spec always opens with the name — so `Origin::Elsewhere` was
/// unreachable in this reader and every direct reference claimed to be a
/// registry package. `Pin` alone would not have caught it: both fields come
/// off the same `@` and only one of them was reading the right string.
#[test]
fn direct_url_does_not_come_from_the_registry() {
    let t = parse("mypkg @ https://example.invalid/mypkg-1.0.whl\n");
    assert_eq!(t.packages[0].origin, Origin::Elsewhere);
    // A name off the index is still a name off the index.
    let t = parse("mypkg==1.0\n");
    assert_eq!(t.packages[0].origin, Origin::Registry);
}

/// What the origin is *for*. `nunpy` is one edit from `numpy` and in no
/// corpus, so the name rules have every reason to shout — and no standing to,
/// because the file says where the bytes come from and it is not PyPI.
/// Before the fix this printed a CRITICAL byte-identical to the `nunpy==1.0`
/// control below it.
#[test]
fn a_direct_reference_is_not_a_slopsquat() {
    let url = parse("nunpy @ https://example.invalid/nunpy-1.0-py3-none-any.whl\n");
    assert!(
        slopsquat::scan(&url, slopsquat::Config::default()).is_empty(),
        "the corpus cannot speak about a name it never covered"
    );

    // The control. If this stops firing the test above proves nothing.
    let pinned = parse("nunpy==1.0\n");
    let found = slopsquat::scan(&pinned, slopsquat::Config::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].package, "nunpy");
}

/// A URL, a VCS reference and a path are all things pip installs and none of
/// them is a name to audit, so they are skipped — one line, not the file.
/// Refusing the file took `requests` down with it, which is the finding
/// somebody actually wanted.
#[test]
fn a_location_requirement_is_skipped_not_fatal() {
    let t = parse(concat!(
        "requests==2.31.0\n",
        "git+https://github.com/psf/requests.git@main#egg=requests\n",
        "https://example.invalid/foo-1.0.tar.gz\n",
        "./local/pkg-1.0.tar.gz\n",
        "/opt/wheels/pkg-1.0-py3-none-any.whl\n",
    ));
    assert_eq!(t.packages.len(), 1, "{:?}", t.packages);
    assert_eq!(t.packages[0].name, "requests");
    assert_eq!(t.packages[0].version, "2.31.0");
}

/// The skip is narrow on purpose. A name that is merely wrong is still an
/// error with a line number on it — `.leading-dot` and `./local/pkg` both
/// fail the same name check, and only one of them is a path.
#[test]
fn skipping_locations_does_not_swallow_malformed_names() {
    let err = pip::read(Path::new("r.txt"), "six==1.16.0\n.leading-dot==1.0\n").unwrap_err();
    assert!(err.to_string().contains("package name"), "{err}");
}

#[test]
fn dangling_backslash_at_eof() {
    let t = parse("six==1.16.0 \\\n");
    assert_eq!(t.packages.len(), 1);
    assert_eq!(t.packages[0].name, "six");
}

#[test]
fn crlf() {
    let t = parse("# c\r\nrequests==2.31.0\r\nsix\r\n");
    assert_eq!(t.packages.len(), 2);
    assert_eq!(t.packages[0].version, "2.31.0");
    assert_eq!(pin_of(&t, "six"), Pin::Unconstrained);
}

#[test]
fn empty_file() {
    let t = parse("");
    assert!(t.packages.is_empty());
    assert!(t.roots.is_empty());
}

/// Every malformed line reports where it is. Without a line number the
/// message is useless on a file with three hundred requirements in it.
#[test]
fn malformed_lines_carry_a_position() {
    for (src, line, needle) in [
        ("six==1.16.0\n==2.0\n", 2, "package name"),
        ("six==1.16.0\n\n.leading-dot==1.0\n", 3, "package name"),
        ("flask[async==3.0\n", 1, "unclosed"),
        ("six==1.16.0\nrequests>>2.0\n", 2, "not a version specifier"),
        (
            "six==1.16.0\nrequests>=1.0,\n",
            2,
            "not a version specifier",
        ),
        ("requests==\n", 1, "not a version specifier"),
    ] {
        let err = pip::read(Path::new("r.txt"), src).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(needle), "{src:?}: {msg}");
        assert!(msg.ends_with(&format!("at {line}:1")), "{src:?}: {msg}");
    }
}

/// The column follows the indent, and a joined line reports the line it
/// started on rather than the one it ended on.
#[test]
fn positions_on_indented_and_joined_lines() {
    let err = pip::read(Path::new("r.txt"), "  ==2.0\n").unwrap_err();
    assert!(err.to_string().ends_with("at 1:3"), "{err}");

    let err = pip::read(Path::new("r.txt"), "six==1.16.0\n>>2.0 \\\n    --hash=x\n").unwrap_err();
    assert!(err.to_string().ends_with("at 2:1"), "{err}");
}

/// The dispatcher has to reach this reader through the fixture naming scheme,
/// where the file is `reqs-s.requirements.txt` and not `requirements.txt`.
#[test]
fn dispatch_by_suffix() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("poisoned.requirements.txt");
    let t = stranger::lock::read(&path).unwrap();
    assert_eq!(t.ecosystem, Ecosystem::PyPi);
    assert_eq!(t.packages.len(), 6);
}

// ---------------------------------------------------------------- pinning

fn unpinned(t: &Tree) -> Vec<(String, Severity)> {
    pinning::scan(t)
        .into_iter()
        .inspect(|f| assert_eq!(f.rule, Rule::Pinning))
        .map(|f| (f.package, f.severity))
        .collect()
}

/// The poisoned fixture line by line. Three of its six requirements name a
/// version and three do not, and the three that do have to stay silent — a
/// rule that flags `requests==2.31.0` is a rule people turn off.
#[test]
fn poisoned_unpinned_only() {
    let found = unpinned(&load("poisoned.requirements.txt"));
    assert_eq!(
        found,
        vec![
            ("flask".to_string(), Severity::Low),
            ("numpy".to_string(), Severity::High),
            ("urllib3".to_string(), Severity::Medium),
        ]
    );
    for pinned in ["requests", "python-dateutils", "requests-http"] {
        assert!(
            !found.iter().any(|(name, _)| name == pinned),
            "{pinned} names a version and must not be flagged"
        );
    }
}

/// The detail quotes the file rather than paraphrasing it, or there is no way
/// to disagree with a finding without opening the file yourself.
#[test]
fn detail_quotes_the_specifier() {
    let findings = pinning::scan(&load("poisoned.requirements.txt"));
    let detail = |name: &str| {
        findings
            .iter()
            .find(|f| f.package == name)
            .unwrap_or_else(|| panic!("no {name}"))
            .detail
            .clone()
    };
    assert!(
        detail("urllib3").starts_with(">=1.26 "),
        "{}",
        detail("urllib3")
    );
    assert!(detail("flask").starts_with("~=3.0 "), "{}", detail("flask"));
    // Nothing to quote for a bare name, so it must not invent a specifier.
    assert!(!detail("numpy").contains('='), "{}", detail("numpy"));
}

/// Every line in `reqs-s` is a `==` pin and every line in `reqs-xs` is a bare
/// name. The two fixtures are this rule's floor and ceiling.
#[test]
fn fixture_extremes() {
    assert!(unpinned(&load("reqs-s.requirements.txt")).is_empty());
    let xs = unpinned(&load("reqs-xs.requirements.txt"));
    assert_eq!(xs.len(), 12);
    assert!(xs.iter().all(|(_, s)| *s == Severity::High));
}

/// A lockfile resolves one version per entry, so there is never anything to
/// report. Bailing on the ecosystem rather than scanning it is what keeps that
/// true if some future reader gets `pinned` wrong.
#[test]
fn other_ecosystems_are_silent() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("npm-xs.package-lock.json");
    let t = stranger::lock::read(&path).unwrap();
    assert!(pinning::scan(&t).is_empty());
}
