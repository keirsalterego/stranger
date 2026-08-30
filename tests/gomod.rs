use std::fs;
use std::path::{Path, PathBuf};
use stranger::error::Error;
use stranger::lock::gomod;
use stranger::lock::{Ecosystem, Origin, Package, Pin, Tree};
use stranger::rules::{drift, pinning, scripts, slopsquat, trivial};

fn path_to(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn load(name: &str) -> Tree {
    let path = path_to(name);
    let src = fs::read_to_string(&path).unwrap();
    gomod::read(&path, &src).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Hand-written input, for the directives no real go.mod on this machine
/// carries.
fn parse(src: &str) -> Tree {
    gomod::read(Path::new("go.mod"), src).unwrap_or_else(|e| panic!("{e}"))
}

fn reject(src: &str) -> Error {
    match gomod::read(Path::new("go.mod"), src) {
        Ok(t) => panic!("should not parse, got {} packages", t.packages.len()),
        Err(e) => e,
    }
}

fn at(src: &str) -> (u32, u32) {
    match reject(src) {
        Error::Syntax { line, col, .. } => (line, col),
        e => panic!("expected a syntax error, got {e}"),
    }
}

fn find<'a>(t: &'a Tree, name: &str) -> &'a Package {
    t.packages
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn is_root(t: &Tree, name: &str) -> bool {
    t.roots.iter().any(|&i| t.packages[i].name == name)
}

/// `grep -cE '[^[:space:]]+\.[^[:space:]]*[[:space:]]+v[0-9]'` — a module path
/// and a version on one line. Not "an indented line", which is the obvious
/// count and the wrong one: `gomod-xs` would gain the two bare versions in its
/// retract block.
#[test]
fn counts() {
    assert_eq!(load("gomod-m.go.mod").packages.len(), 174);
    assert_eq!(load("gomod-xs.go.mod").packages.len(), 6);
}

/// The format records no edges. Asserted rather than assumed, because the
/// slopsquat rule reads `in_degree` and would otherwise lean on a fact nobody
/// checked — and because a reader that invented edges out of `// indirect`
/// would claim a graph this file does not contain.
#[test]
fn flat() {
    for name in ["gomod-m.go.mod", "gomod-xs.go.mod"] {
        let t = load(name);
        assert_eq!(t.ecosystem, Ecosystem::Go);
        assert!(t.edges.is_empty(), "{name} grew an edge");
        assert!(t.in_degree().iter().all(|&d| d == 0), "{name}");
    }
}

/// `// indirect` is the whole graph signal, so it had better be the whole
/// direct/transitive split too. 124 of `gomod-m`'s 174 lines carry the marker.
#[test]
fn indirect_is_the_split() {
    let t = load("gomod-m.go.mod");
    assert_eq!(t.direct(), 50);
    assert_eq!(t.transitive(), 124);
    assert!(is_root(&t, "github.com/pkg/errors"));
    assert!(!is_root(&t, "github.com/miekg/dns"));
}

/// `require x v1.0.0 // indirect` on one line, which is the form a small
/// module uses and neither require block in `gomod-m` contains.
#[test]
fn single_line_require() {
    let t = load("gomod-xs.go.mod");
    assert_eq!(t.direct(), 5);
    assert_eq!(t.transitive(), 1);
    assert!(!is_root(&t, "golang.org/x/text"));
    assert_eq!(find(&t, "golang.org/x/text").version, "v0.23.0");
}

/// A retract block holds this module's own withdrawn versions. Read as
/// requirements they would be four packages named `v1.4.1` and friends.
#[test]
fn retracted_versions_are_not_packages() {
    let t = load("gomod-xs.go.mod");
    assert!(!t.packages.iter().any(|p| p.name.starts_with('v')));
}

#[test]
fn versions_are_kept_as_written() {
    let t = load("gomod-m.go.mod");
    assert_eq!(
        find(&t, "github.com/logrusorgru/aurora").version,
        "v2.0.3+incompatible"
    );
    assert_eq!(
        find(&t, "github.com/hbakhtiyor/strsim").version,
        "v0.0.0-20190107154042-4d2bbb273edf"
    );
    // Both escapes on one line: a pseudo-version on a module that never
    // adopted modules, so the major lives in the tag rather than the path.
    assert_eq!(
        find(&t, "github.com/Knetic/govaluate").version,
        "v3.0.1-0.20171022003610-9aa49832a739+incompatible"
    );
    assert!(t.packages.iter().all(|p| p.version.starts_with('v')));
}

/// Every directive form in one file, including the four that carry no module
/// path and the block spellings of the ones that do.
#[test]
fn every_directive() {
    let t = parse(
        "\
module example.com/app

go 1.24.2
toolchain go1.24.3
godebug default=go1.24
ignore ./generated

require github.com/one/direct v1.0.0

require (
\t// a comment inside a block, which retract blocks are full of
\tgithub.com/two/blocked v2.1.0
\tgithub.com/three/quiet v0.4.0 // indirect
\tgithub.com/four/noted v1.2.3 // indirect; kept for the linter
)

exclude github.com/two/blocked v2.0.9

exclude (
\tgithub.com/one/direct v0.9.0
)

replace github.com/three/quiet => github.com/three/quiet v0.4.1

replace (
\tgithub.com/four/noted v1.2.3 => ../vendored/noted
)

retract v0.1.0

retract (
\tv0.2.0 // withdrawn
\t[v0.3.0, v0.4.0]
)

tool (
\tgithub.com/two/blocked/cmd/gen
)
",
    );

    let names: Vec<&str> = t.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "github.com/one/direct",
            "github.com/two/blocked",
            "github.com/three/quiet",
            "github.com/four/noted",
        ]
    );
    // Direct, in file order, minus the one a replace made first-party.
    assert_eq!(t.direct(), 2);
    assert_eq!(t.transitive(), 1);
    assert_eq!(t.workspace_members(), 1);
    // `; kept for the linter` is what `go mod tidy` writes when the line
    // already had a comment, and it still means indirect.
    assert!(!is_root(&t, "github.com/four/noted"));
}

/// A replacement pointing at a directory is code in the tree being audited —
/// npm's `link: true` in another grammar. One pointing at another module is
/// still somebody else's, but it is not the module the corpus would be asked
/// about, so its origin stops being the registry.
#[test]
fn replace_decides_first_party_and_origin() {
    let t = parse(
        "\
module example.com/app
go 1.24
require (
\tgithub.com/us/internal v1.0.0
\tgithub.com/them/forked v2.0.0
\tgithub.com/them/plain v3.0.0
)
replace github.com/us/internal => ./internal
replace github.com/them/forked v2.0.0 => github.com/fork/of-it v2.0.1
",
    );
    assert!(find(&t, "github.com/us/internal").first_party);
    assert_eq!(find(&t, "github.com/us/internal").origin, Origin::Elsewhere);
    assert!(!is_root(&t, "github.com/us/internal"));

    assert!(!find(&t, "github.com/them/forked").first_party);
    assert_eq!(find(&t, "github.com/them/forked").origin, Origin::Elsewhere);
    assert_eq!(find(&t, "github.com/them/plain").origin, Origin::Registry);
}

/// go.mod records none of these, and the reader says so rather than reporting
/// a clean `false` it never checked — except `install_script`, where `false`
/// is the measurement: the module system has no install-time hook.
#[test]
fn nothing_claims_dev_scripts_or_hashes() {
    let t = load("gomod-m.go.mod");
    assert!(!t.packages.iter().any(|p| p.dev || p.optional));
    assert!(!t.packages.iter().any(|p| p.install_script));
    assert!(!t.packages.iter().any(|p| p.has_integrity));
    assert!(t.packages.iter().all(|p| p.pinned == Pin::Exact));
}

#[test]
fn unknown_directives_are_refused() {
    // The go team keeps adding directives. Skipping the ones we have not
    // heard of would one day skip a `require` spelled slightly wrong.
    assert_eq!(
        at("module m\nrequire x.io/y v1.0.0\nprovides x.io/z\n"),
        (3, 1)
    );
    assert_eq!(at("module m\n  /* not a go.mod comment */\n"), (2, 3));
}

#[test]
fn garbage_versions_are_refused() {
    for src in [
        "module m\nrequire example.com/x 1.2.3\n",
        "module m\nrequire example.com/x v1.2\n",
        "module m\nrequire example.com/x v1.2.3.4\n",
        "module m\nrequire example.com/x vlatest\n",
        "module m\nrequire example.com/x v1.2.x\n",
    ] {
        // Column 23 is the version, not the line: `require ` is eight
        // characters and `example.com/x ` is fourteen more.
        assert_eq!(at(src), (2, 23), "{src:?}");
    }
    // Arity, which is what a version that went missing looks like.
    assert_eq!(at("module m\nrequire example.com/x\n"), (2, 1));
    assert_eq!(at("module m\nrequire (\n\texample.com/x\n)\n"), (3, 2));
}

/// Truncated at the wrong moment — a download that stopped, or a merge that
/// took half a block. The position is where the block opened, because that is
/// the line worth opening.
#[test]
fn an_unterminated_block_points_at_the_open() {
    assert_eq!(
        at("module m\n\nrequire (\n\texample.com/x v1.0.0\n"),
        (3, 1)
    );
    assert_eq!(at("module m\nrequire (\n"), (2, 1));
}

#[test]
fn a_file_that_is_not_a_go_mod_is_refused() {
    // No module directive: the same guard the Cargo reader puts on a TOML
    // file with no `[[package]]`.
    let err = reject("go 1.24\nrequire example.com/x v1.0.0\n");
    assert!(err.to_string().contains("no `module` directive"), "{err}");
    assert!(gomod::read(Path::new("go.mod"), "").is_err());
}

#[test]
fn stray_punctuation_is_refused() {
    assert_eq!(at("module m\n)\n"), (2, 1));
    assert_eq!(
        at("module m\nrequire (\n\texample.com/x v1.0.0\n) trailing\n"),
        (4, 3)
    );
    assert_eq!(at("module m\nrequire ( example.com/x v1.0.0 )\n"), (2, 11));
    // An opening quote with no closing one is a real syntax error, and stays
    // one now that terminated quotes are read.
    assert_eq!(at("module m\nrequire \"example.com/x v1.0.0\n"), (2, 9));
}

/// A quoted module path is ordinary go.mod, not a corner of it: `gopkg.in/
/// yaml.v3` ships one, and this reader used to refuse the file outright with
/// a comment claiming nothing in the wild wrote them.
///
/// Both string forms, in both the directive and the block, and the quotes
/// must come off the path rather than travel with it — a package named
/// `"example.com/x"` would be absent from every corpus and score as a
/// stranger on the strength of its own punctuation.
#[test]
fn quoted_module_paths_are_read() {
    let t = parse(concat!(
        "module \"example.com/m\"\n",
        "go 1.21\n",
        "require (\n",
        "\t\"example.com/x\" v1.0.0\n",
        "\t`example.com/y` v2.0.0 // indirect\n",
        ")\n",
        "require \"example.com/z\" v3.0.0\n",
    ));
    let mut names: Vec<&str> = t.packages.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["example.com/x", "example.com/y", "example.com/z"]);
    assert!(t.packages.iter().all(|p| !p.name.contains(['"', '`'])));
    // `// indirect` still lands, so unquoting did not eat the comment split.
    assert_eq!(t.roots.len(), 2);
}

/// The one quoted path that stays refused. A Go string escape cannot survive
/// the module proxy, which URL-encodes the path it fetches, so a path needing
/// one resolves to nothing — and reading `\n` as two characters would invent
/// a package name no registry holds.
#[test]
fn an_escaped_path_is_still_refused() {
    let err = reject("module m\nrequire \"example.com/\\x\" v1.0.0\n");
    assert!(err.to_string().contains("string escape"), "{err}");
}

/// The real file that exposed the bug, byte for byte.
#[test]
fn the_yaml_v3_header_parses() {
    let t = parse(concat!(
        "module \"gopkg.in/yaml.v3\"\n",
        "\n",
        "require (\n",
        "\t\"gopkg.in/check.v1\" v0.0.0-20161208181325-20d25e280405\n",
        ")\n",
    ));
    assert_eq!(t.packages.len(), 1);
    assert_eq!(t.packages[0].name, "gopkg.in/check.v1");
}

/// Fixtures are named `gomod-m.go.mod`, so dispatch has to match on the suffix
/// and not be shadowed by the six arms above it.
#[test]
fn dispatch_matches_the_suffix() {
    let t = stranger::lock::read(&path_to("gomod-m.go.mod")).unwrap();
    assert_eq!(t.ecosystem, Ecosystem::Go);
    assert_eq!(t.packages.len(), 174);
}

/// The rule that must never fire here. There is no ranked list of Go module
/// paths to be absent from, so "not in the corpus" would mean "not in a list
/// nobody publishes" and every module in this fixture would be a candidate.
///
/// The second half is what makes that structural. Hand the rule a corpus
/// containing a one-edit neighbour of a real module in the tree — the exact
/// shape that fires CRITICAL on npm — and it still says nothing, because the
/// guard is on the ecosystem's own corpus and not on the one passed in.
#[test]
fn slopsquat_stays_silent_on_go() {
    let t = load("gomod-m.go.mod");
    assert!(slopsquat::scan(&t, Default::default()).is_empty());

    let bait = ["github.com/pkg/error"];
    assert!(
        slopsquat::scan(
            &t,
            slopsquat::Config {
                require_no_parent: true,
                corpus: Some(&bait),
            }
        )
        .is_empty(),
        "the corpus parameter switched the rule on for an ecosystem with no corpus"
    );
}

/// And nothing else fires either, which is the honest shape of a Go scan: the
/// tree is read, the split is read, and the report is empty.
///
/// Three of these four cannot fire on this format at all. `trivial` can, in
/// principle — it matches the last path segment, so a module ending `/is-foo`
/// with no edges out of it would be reported — which is why it is asserted
/// against the fixtures rather than argued away.
#[test]
fn a_go_scan_reports_nothing() {
    for name in ["gomod-m.go.mod", "gomod-xs.go.mod"] {
        let t = load(name);
        assert!(drift::scan(&t).is_empty(), "{name}");
        assert!(pinning::scan(&t).is_empty(), "{name}");
        assert!(scripts::scan(&t).is_empty(), "{name}");
        assert!(trivial::scan(&t).is_empty(), "{name}");
    }
}
