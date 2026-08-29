use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use stranger::error::Error;
use stranger::yaml::{self, Value};

fn parse(s: &str) -> Value {
    yaml::parse(s).unwrap_or_else(|e| panic!("{s:?} should parse: {e}"))
}

fn reject(s: &str) -> Error {
    match yaml::parse(s) {
        Ok(v) => panic!("{s:?} should not parse, got {v:?}"),
        Err(e) => e,
    }
}

fn at(s: &str) -> (u32, u32) {
    match reject(s) {
        Error::Syntax { line, col, .. } => (line, col),
        e => panic!("expected a syntax error, got {e}"),
    }
}

fn why(s: &str) -> String {
    match reject(s) {
        Error::Syntax { what, .. } => what,
        e => panic!("expected a syntax error, got {e}"),
    }
}

fn s(v: &str) -> Value {
    Value::String(v.into())
}

fn fixture() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("pnpm-l.pnpm-lock.yaml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn scalars() {
    assert_eq!(parse("a: hi").get("a"), Some(&s("hi")));
    assert_eq!(parse("a: 'hi'").get("a"), Some(&s("hi")));
    assert_eq!(parse(r#"a: "hi""#).get("a"), Some(&s("hi")));
    assert_eq!(parse("a: true").get("a"), Some(&Value::Bool(true)));
    assert_eq!(parse("a: false").get("a"), Some(&Value::Bool(false)));
    assert_eq!(parse("a:").get("a"), Some(&Value::Null));
    // Trailing spaces are not part of the value.
    assert_eq!(parse("a: hi   \n").get("a"), Some(&s("hi")));
}

/// The whole implicit-typing decision, as a test. Everything here is a string
/// and stays a string; a YAML 1.1 parser turns half of it into something else.
#[test]
fn nothing_but_true_and_false_is_typed() {
    for text in [
        "no", "NO", "No", "yes", "on", "off", "y", "n", "True", "FALSE", "null", "Null", "~",
        "9.0", "1.0", "010", "0x10", "1e3", "-1", ".inf",
    ] {
        let doc = parse(&format!("a: {text}"));
        assert_eq!(doc.get("a"), Some(&s(text)), "{text} should stay a string");
    }
}

/// `Value::Null` means "this key had no value", never the word null.
#[test]
fn null_comes_from_an_empty_value_only() {
    assert_eq!(parse("a:").get("a"), Some(&Value::Null));
    assert_eq!(parse("a: null").get("a"), Some(&s("null")));
    assert_eq!(parse(""), Value::Null);
    assert_eq!(parse("\n\n  \n"), Value::Null);
    assert_eq!(parse("# just a comment\n"), Value::Null);
}

#[test]
fn nested_mappings() {
    let doc = parse("a:\n  b:\n    c: 1\n  d: 2\ne: 3\n");
    assert_eq!(
        doc.get("a")
            .and_then(|v| v.get("b"))
            .and_then(|v| v.get("c")),
        Some(&s("1"))
    );
    assert_eq!(doc.get("a").and_then(|v| v.get("d")), Some(&s("2")));
    assert_eq!(doc.get("e"), Some(&s("3")));
}

/// A blank line between two entries of the same block is invisible to
/// structure. The fixture has 1,705 of them; getting this wrong reads one
/// package and calls the project clean.
#[test]
fn blank_and_comment_lines_do_not_close_a_block() {
    let doc = parse("a:\n  b: 1\n\n\n  # note\n\n  c: 2\nd: 3\n");
    let a = doc.get("a").expect("a");
    assert_eq!(a.get("b"), Some(&s("1")));
    assert_eq!(a.get("c"), Some(&s("2")));
    assert_eq!(doc.get("d"), Some(&s("3")));
}

#[test]
fn sequences() {
    let doc = parse("a:\n  - one\n  - two\n");
    assert_eq!(
        doc.get("a").and_then(Value::as_sequence),
        Some(&[s("one"), s("two")][..])
    );
    // A sequence closes when the indentation drops back.
    let doc = parse("a:\n  - one\nb: 2\n");
    assert_eq!(
        doc.get("a").and_then(Value::as_sequence).map(<[_]>::len),
        Some(1)
    );
    assert_eq!(doc.get("b"), Some(&s("2")));
}

#[test]
fn flow_collections() {
    assert_eq!(
        parse("a: {}")
            .get("a")
            .and_then(Value::as_mapping)
            .map(|m| m.len()),
        Some(0)
    );
    assert_eq!(parse("a: []").get("a"), Some(&Value::Sequence(vec![])));
    assert_eq!(
        parse("a: [x, y]").get("a").and_then(Value::as_sequence),
        Some(&[s("x"), s("y")][..])
    );
    let doc = parse("a: {b: 1, c: 'two'}");
    assert_eq!(doc.get("a").and_then(|v| v.get("b")), Some(&s("1")));
    assert_eq!(doc.get("a").and_then(|v| v.get("c")), Some(&s("two")));
    // Nested, which the fixture never does but the grammar allows.
    let doc = parse("a: {b: [1, {c: 2}]}");
    assert!(doc.get("a").and_then(|v| v.get("b")).is_some());
}

/// The two shapes 1,570 of the fixture's lines are made of. Both hold
/// characters a careless scanner treats as structure.
#[test]
fn flow_values_from_the_fixture() {
    let doc = parse("resolution: {integrity: sha512-30iZ+LT/Yw==}");
    assert_eq!(
        doc.get("resolution").and_then(|v| v.get("integrity")),
        Some(&s("sha512-30iZ+LT/Yw=="))
    );
    let doc = parse("engines: {node: ^18.17.1 || ^20.3.0 || >=22.0.0}");
    assert_eq!(
        doc.get("engines").and_then(|v| v.get("node")),
        Some(&s("^18.17.1 || ^20.3.0 || >=22.0.0"))
    );
    let doc = parse("engines: {iojs: '>=1.0.0', node: '>=0.10.0'}");
    assert_eq!(
        doc.get("engines").and_then(|v| v.get("iojs")),
        Some(&s(">=1.0.0"))
    );
}

/// The same node, two legal spellings. pnpm writes the brace on the key's
/// line; a reformatter is free to move it down one, and YAML says they mean
/// the same thing.
///
/// Read as a block mapping, the second spelling produced the key `{integrity`
/// — so the package had no integrity hash, its origin fell to `Elsewhere`,
/// and the slopsquat rule skips `Elsewhere` by design. Pressing save in an
/// editor turned findings off. `tests/pnpm.rs` checks the findings end of it.
#[test]
fn a_flow_collection_may_sit_under_its_key() {
    assert_eq!(
        parse("resolution:\n  {integrity: sha512-AAAA}\n"),
        parse("resolution: {integrity: sha512-AAAA}\n")
    );
    assert_eq!(
        parse("os:\n  [darwin, linux]\n"),
        parse("os: [darwin, linux]\n")
    );
    // Deeper than the key, like anything else in a block, and it closes the
    // moment the line ends.
    let doc = parse("a:\n  {b: 1}\nc: 2\n");
    assert_eq!(doc.get("a").and_then(|v| v.get("b")), Some(&s("1")));
    assert_eq!(doc.get("c"), Some(&s("2")));
    // Still one line: moving the brace down does not make it foldable.
    assert_eq!(
        why("a:\n  {b: 1\n  }\n"),
        "a flow mapping may not span lines"
    );
    // And a second one under the same key has no block to be in.
    assert_eq!(why("a:\n  {b: 1}\n  {c: 2}\n"), "unexpected indentation");
}

/// `{a: 1}: 2` is a flow mapping used as a *key*, which this subset does not
/// have. It came back as the key `{a` — the same invented-key bug as the
/// own-line resolution, reached from the other side.
#[test]
fn a_flow_indicator_may_not_open_a_plain_scalar() {
    let msg = "a flow indicator may not open a plain scalar; quote it";
    assert_eq!(why("a: 1\n{b: 2}: 3\n"), msg);
    assert_eq!(at("a: 1\n{b: 2}: 3\n"), (2, 1));
    assert_eq!(why("a: 1\n[b]: 3\n"), msg);
    assert_eq!(why("a: }x\n"), msg);
    assert_eq!(why("a: ,x\n"), msg);
    // Quoting is how you say you meant the text.
    assert_eq!(parse("a: 1\n'{b: 2}': 3\n").get("{b: 2}"), Some(&s("3")));
    // Inside a flow collection these five bytes are structure, and the flow
    // parsers keep saying the more useful thing about them.
    assert_eq!(why("a: [,]"), "expected a value");
    assert_eq!(why("a: {,}"), "expected ':' after a flow mapping key");
}

/// Of the four scanners that read unquoted text, the flow key was the one
/// that never checked for an indicator — so a flow mapping was the way to get
/// an anchor past a parser that refuses anchors everywhere else.
#[test]
fn a_flow_key_refuses_indicators() {
    assert_eq!(
        why("a: {&x b: 1}"),
        "anchors are not part of the supported YAML subset"
    );
    assert_eq!(at("a: {&x b: 1}"), (1, 5));
    assert_eq!(
        why("a: {*x: 1}"),
        "aliases are not part of the supported YAML subset"
    );
    assert_eq!(
        why("a: {!!str b: 1}"),
        "tags are not part of the supported YAML subset"
    );
    assert_eq!(
        why("a: {@x: 1}"),
        "this character is reserved in YAML; quote the scalar"
    );
    // Quoted, it is a key like any other — which is what pnpm writes.
    assert!(parse("a: {'@scope/pkg': 1}").get("a").is_some());
}

/// pnpm keys are package identifiers. `split(':')` cuts three of these in the
/// wrong place.
#[test]
fn keys_that_break_a_naive_split() {
    assert_eq!(parse("zwitch@2.0.4: 1").get("zwitch@2.0.4"), Some(&s("1")));
    assert_eq!(parse(".: 1").get("."), Some(&s("1")));
    assert!(
        parse("acorn-jsx@5.3.2(acorn@8.14.1): {}")
            .get("acorn-jsx@5.3.2(acorn@8.14.1)")
            .is_some()
    );
    assert_eq!(
        parse("'@babel/core@7.27.1': 1").get("@babel/core@7.27.1"),
        Some(&s("1"))
    );
    // A colon inside a plain key is only a key/value separator when a space
    // follows it.
    assert_eq!(parse("a:b: 1").get("a:b"), Some(&s("1")));
    assert_eq!(parse("http://x: 1").get("http://x"), Some(&s("1")));
}

#[test]
fn quoted_scalars() {
    assert_eq!(parse("a: 'it''s'").get("a"), Some(&s("it's")));
    assert_eq!(
        parse("a: 'SECURITY: fixed'").get("a"),
        Some(&s("SECURITY: fixed"))
    );
    assert_eq!(
        parse("a: '#notacomment'").get("a"),
        Some(&s("#notacomment"))
    );
    assert_eq!(parse(r#"a: "x\ny""#).get("a"), Some(&s("x\ny")));
    assert_eq!(parse(r#"a: "\u00e9""#).get("a"), Some(&s("é")));
    assert_eq!(parse(r#"a: "\"\\\/""#).get("a"), Some(&s("\"\\/")));
}

#[test]
fn comments() {
    assert_eq!(parse("a: 1 # note").get("a"), Some(&s("1")));
    assert!(parse("a: {b: 1} # note").get("a").is_some());
    // A `#` with no space before it is part of the scalar, which is what YAML
    // says and what keeps `sha512-a#b` intact.
    assert_eq!(parse("a: x#y").get("a"), Some(&s("x#y")));
    assert!(
        parse("a: {b: x#y}")
            .get("a")
            .and_then(|v| v.get("b"))
            .is_some()
    );
}

/// A flow key used to be the one scalar scanner that read ` #` as key text,
/// so `{b #x: 1}` came back as the key "b #x" instead of a refusal.
#[test]
fn a_comment_ends_a_flow_key() {
    assert_eq!(why("a: {b #x: 1}"), "expected ':' after a flow mapping key");
    assert_eq!(at("a: {b #x: 1}"), (1, 7));
    // The value side of the same line already refused it.
    assert_eq!(why("a: {b: 1 #x}"), "expected ',' or '}'");
    assert_eq!(at("a: {b: 1 #x}"), (1, 10));
}

/// A lockfile checked out on Windows. `\r\n` is a line ending; a lone `\r` is
/// not, and is refused rather than folded into the scalar before it.
#[test]
fn crlf_line_endings() {
    let doc = parse("a:\r\n  b: 1\r\n  c: [x, y]\r\nd: 2\r\n");
    assert_eq!(doc.get("a").and_then(|v| v.get("b")), Some(&s("1")));
    assert_eq!(
        doc.get("a")
            .and_then(|v| v.get("c"))
            .and_then(Value::as_sequence),
        Some(&[s("x"), s("y")][..])
    );
    assert_eq!(doc.get("d"), Some(&s("2")));
    assert_eq!(parse("a: 1\r\n\r\nb: 'q'\r\n").get("b"), Some(&s("q")));
    reject("a: 1\rb: 2\r");
}

// -- refusals ---------------------------------------------------------------

/// YAML forbids tabs in indentation, and an editor renders them as if they
/// were fine. This is the one whitespace bug that has to come back with a
/// column.
#[test]
fn tabs_in_indentation_are_refused() {
    assert_eq!(why("a:\n\tb: 1\n"), "tab used for indentation");
    assert_eq!(at("a:\n\tb: 1\n"), (2, 1));
    assert_eq!(at("a:\n  b:\n   \tc: 1\n"), (3, 4));
    // Even on a line that carries nothing else.
    assert_eq!(at("a: 1\n\t\n"), (2, 1));
    // A tab *after* the indentation is separation whitespace and is legal.
    assert_eq!(parse("a:\tb").get("a"), Some(&s("b")));
}

/// A dedent has to land on a level that is actually open. Landing between two
/// of them is the classic hand-edited-lockfile bug, and a forgiving parser
/// silently reparents the key.
#[test]
fn dedent_must_land_on_an_open_level() {
    assert_eq!(why("a:\n    b: 1\n  c: 2\n"), "unexpected indentation");
    assert_eq!(at("a:\n    b: 1\n  c: 2\n"), (3, 3));
    // Deeper than anything that opened a block.
    assert_eq!(at("a: 1\n  b: 2\n"), (2, 3));
    // And the sibling after a nested block is *not* eaten by the dedent.
    let doc = parse("a:\n    b: 1\nc: 2\n");
    assert_eq!(doc.get("c"), Some(&s("2")));
    assert_eq!(doc.get("a").and_then(|v| v.get("b")), Some(&s("1")));
}

#[test]
fn unterminated_quotes() {
    assert_eq!(why("a: 'x\n"), "unterminated single-quoted scalar");
    assert_eq!(at("a: 'x\n"), (1, 4));
    assert_eq!(why("a: \"x\n"), "unterminated double-quoted scalar");
    assert_eq!(why("'a: 1\n"), "unterminated single-quoted scalar");
    // A quoted scalar that runs to end of file, not just end of line.
    assert_eq!(why("a: 'x"), "unterminated single-quoted scalar");
}

#[test]
fn unclosed_flow_collections() {
    assert_eq!(why("a: {b: 1\n"), "a flow mapping may not span lines");
    assert_eq!(at("a: {b: 1\n"), (1, 4));
    assert_eq!(why("a: [1, 2\n"), "a flow sequence may not span lines");
    assert_eq!(why("a: {b: 1"), "unclosed flow mapping");
    assert_eq!(why("a: [1"), "unclosed flow sequence");
    // Spread over lines is refused rather than folded.
    assert_eq!(
        why("a: {\n  b: 1\n}\n"),
        "a flow mapping may not span lines"
    );
    // Broken after the comma, which is the shape a hand-wrapped `os:` list
    // takes. The mapping path said this by name already; the sequence path
    // fell through to the scalar scanner and said "expected a value".
    assert_eq!(why("a: [x,\ny]"), "a flow sequence may not span lines");
    assert_eq!(at("a: [x,\ny]"), (1, 4));
    assert_eq!(why("a: {b: 1,\nc: 2}"), "a flow mapping may not span lines");
    assert_eq!(at("a: {b: 1,\nc: 2}"), (1, 4));
    assert_eq!(why("a: [x,"), "unclosed flow sequence");
    assert_eq!(why("a: {b: 1,"), "unclosed flow mapping");
}

/// A trailing comma is legal YAML in both flow collections, and both used to
/// be refused with a message that did not say why. `toml.rs` makes the same
/// call for arrays; the difference there is that TOML forbids it in an inline
/// table and YAML forbids it nowhere.
#[test]
fn trailing_commas_in_flow_collections() {
    assert_eq!(
        parse("a: [x, ]").get("a").and_then(Value::as_sequence),
        Some(&[s("x")][..])
    );
    assert_eq!(
        parse("a: [x,]")
            .get("a")
            .and_then(Value::as_sequence)
            .map(<[_]>::len),
        Some(1)
    );
    let doc = parse("a: {b: 1, }");
    assert_eq!(doc.get("a").and_then(|v| v.get("b")), Some(&s("1")));
    assert_eq!(
        doc.get("a").and_then(Value::as_mapping).map(|m| m.len()),
        Some(1)
    );
    // Empty is still empty, and a comma on its own is still not an entry.
    assert_eq!(parse("a: [ ]").get("a"), Some(&Value::Sequence(vec![])));
    assert_eq!(why("a: [,]"), "expected a value");
    assert_eq!(why("a: {,}"), "expected ':' after a flow mapping key");
    assert_eq!(why("a: [x,,]"), "expected a value");
}

/// An unquoted `: ` inside a value is invalid YAML, and the alternative is a
/// string that looks like it parsed.
#[test]
fn plain_scalars_refuse_a_bare_colon() {
    assert_eq!(at("a: b: c\n"), (1, 5));
    assert!(why("a: b: c\n").starts_with("':' in a plain scalar"));
    assert!(why("a: {b: c: d}\n").starts_with("':' in a flow scalar"));
    // Which is what turns a mapping inside a sequence item into a refusal.
    assert!(why("a:\n  - b: 1\n").starts_with("':' in a plain scalar"));
}

#[test]
fn out_of_subset_constructs() {
    // Block scalars.
    assert_eq!(
        why("a: |\n  text\n"),
        "block scalars are not part of the supported YAML subset"
    );
    assert!(!why("a: >-\n  text\n").is_empty());
    // A sequence level with its key.
    assert_eq!(
        why("a:\n- one\n"),
        "a block sequence must be indented under its key, not level with it"
    );
    // The plain-scalar scanner would happily read "&anchor 1" as a string, so
    // the indicators are refused by name and the message says which.
    assert_eq!(
        why("a: &anchor 1\nb: *anchor\n"),
        "anchors are not part of the supported YAML subset"
    );
    assert_eq!(at("a: &anchor 1\n"), (1, 4));
    assert_eq!(
        why("a: 1\nb: *anchor\n"),
        "aliases are not part of the supported YAML subset"
    );
    assert_eq!(
        why("a: !!str 1\n"),
        "tags are not part of the supported YAML subset"
    );
    assert_eq!(
        why("%YAML 1.2\n---\na: 1\n"),
        "directives are not part of the supported YAML subset"
    );
    // An unquoted `@` is reserved in YAML, which is why pnpm quotes every
    // scoped package key.
    assert!(!why("@scope/pkg: 1\n").is_empty());
    // A second document.
    reject("a: 1\n---\nb: 2\n");
    // A merge key is an alias in value position.
    reject("a: &x {b: 1}\nc:\n  <<: *x\n");
}

/// One `---` opens the document and is skipped; everything else about
/// documents is refused by name. Before this, `--- a: 1` parsed to the key
/// "--- a" and the legal `---\na: 1` was refused with a message that named
/// nothing.
#[test]
fn document_markers() {
    assert_eq!(parse("---\na: 1\n").get("a"), Some(&s("1")));
    assert_eq!(parse("--- # note\na: 1\n").get("a"), Some(&s("1")));
    assert_eq!(parse("# note\n---\na: 1\n").get("a"), Some(&s("1")));
    assert_eq!(parse("\u{feff}---\na: 1\n").get("a"), Some(&s("1")));
    assert_eq!(parse("---\n"), Value::Null);

    assert_eq!(
        why("--- a: 1\n"),
        "a document marker must be alone on its line"
    );
    assert_eq!(at("--- a: 1\n"), (1, 5));
    assert_eq!(
        why("a: 1\n---\nb: 2\n"),
        "a second document is not part of the supported YAML subset"
    );
    assert_eq!(at("a: 1\n---\nb: 2\n"), (2, 1));
    assert_eq!(
        why("---\n---\na: 1\n"),
        "a second document is not part of the supported YAML subset"
    );
    assert_eq!(
        why("a: 1\n...\n"),
        "a document end marker is not part of the supported YAML subset"
    );
    assert_eq!(at("a: 1\n...\n"), (2, 1));
    // Neither is a marker: the break after the three characters is what
    // decides, and an indented `---` is a scalar, not structure.
    assert_eq!(parse("---foo: 1\n").get("---foo"), Some(&s("1")));
    assert_eq!(parse("a: ---\n").get("a"), Some(&s("---")));
    assert_eq!(at("a:\n  ---\n"), (2, 3));
}

#[test]
fn duplicate_keys_are_an_error() {
    assert_eq!(why("a: 1\na: 2\n"), "duplicate key `a`");
    assert_eq!(at("a: 1\na: 2\n"), (2, 1));
    assert_eq!(why("x: {a: 1, a: 2}\n"), "duplicate key `a`");
}

#[test]
fn malformed_lines() {
    assert_eq!(why("a\n"), "expected ':' after a mapping key");
    assert_eq!(why(": 1\n"), "empty mapping key");
    assert!(!why("a: 1 junk: 2\n").is_empty());
    assert_eq!(why("a:\n  - \n"), "expected a value after '-'");
    assert_eq!(why("'a' b: 1\n"), "expected ':' after a mapping key");
    assert_eq!(
        why("a: - b\n"),
        "a sequence item may not start on its parent's line"
    );
}

/// Same as `toml.rs` and `json.rs`: the mark is skipped and does not shift
/// the columns of the line it sits on.
#[test]
fn leading_byte_order_mark() {
    assert_eq!(parse("\u{feff}a: 1\n").get("a"), Some(&s("1")));
    assert_eq!(at("\u{feff}a: b: c\n"), (1, 5));
    assert_eq!(at("a: b: c\n"), (1, 5));
}

#[test]
fn error_positions_point_at_the_problem() {
    // The `:` that should not be there, at character 5 of line 1.
    assert_eq!(at("a: b: c\n"), (1, 5));
    // Column counts characters, so a multi-byte key does not skew it.
    assert_eq!(at("π: b: c\n"), (1, 5));
    assert_eq!(at("a:\n  b: 1\n   c: 2\n"), (3, 4));
}

// -- the real file ----------------------------------------------------------

#[test]
fn the_fixture_parses() {
    let doc = parse(&fixture());
    let top = doc.as_mapping().expect("a mapping at the top");
    assert_eq!(
        top.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "importers",
            "lockfileVersion",
            "packages",
            "settings",
            "snapshots"
        ]
    );
    assert_eq!(doc.get("lockfileVersion"), Some(&s("9.0")));
    assert_eq!(
        doc.get("packages")
            .and_then(Value::as_mapping)
            .map(|m| m.len()),
        Some(850)
    );
    assert_eq!(
        doc.get("snapshots")
            .and_then(Value::as_mapping)
            .map(|m| m.len()),
        Some(850)
    );
    // The bool the whole implicit-typing exception exists for.
    assert_eq!(
        doc.get("settings").and_then(|v| v.get("autoInstallPeers")),
        Some(&Value::Bool(true))
    );
    // A quoted key, a flow mapping, a flow sequence and a block sequence, in
    // the shapes the file actually uses.
    let pkgs = doc.get("packages").expect("packages");
    assert!(pkgs.get("@babel/core@7.27.1").is_some());
    assert!(
        pkgs.get("@ampproject/remapping@2.3.0")
            .and_then(|v| v.get("resolution"))
            .and_then(|v| v.get("integrity"))
            .and_then(Value::as_str)
            .is_some_and(|s| s.starts_with("sha512-"))
    );
    let deprecated = pkgs
        .as_mapping()
        .expect("a mapping")
        .values()
        .filter(|v| v.get("deprecated").is_some())
        .count();
    assert_eq!(deprecated, 3);
}

/// Cut the real file short and parse the prefix. Every prefix is either a
/// valid document or a positioned error; none of them may panic. A truncated
/// lockfile is not hypothetical — it is what a killed `pnpm install` leaves.
///
// ponytail: every offset in the first 8 KB, then every 1,024th. Truncation is
// quadratic and the file is 254 KB, so the exhaustive sweep is about twenty
// minutes in a debug build for coverage of the same handful of constructs. The
// dense window covers `importers` and the start of `packages`, which is where
// the shapes change; the stride covers the repetitive 246 KB after it. Drop
// the stride if a crash ever turns up between two samples.
#[test]
fn truncation_never_panics() {
    let full = fixture();
    for n in 0..full.len().min(8 * 1024) {
        if full.is_char_boundary(n) {
            let _ = yaml::parse(&full[..n]);
        }
    }
    let mut n = 8 * 1024;
    while n < full.len() {
        if full.is_char_boundary(n) {
            let _ = yaml::parse(&full[..n]);
        }
        n += 1024;
    }
}

/// Ten thousand open brackets is a stack overflow in a naive recursive
/// descent parser. Block nesting gets there more slowly but gets there.
#[test]
fn deep_nesting_errors_rather_than_overflowing() {
    reject(&format!("a: {}", "[".repeat(10_000)));
    reject(&format!("a: {}", "{b: ".repeat(10_000)));

    let mut block = String::new();
    for depth in 0..400 {
        block.push_str(&" ".repeat(depth));
        block.push_str("k:\n");
    }
    assert!(why(&block).starts_with("nesting deeper than"));
}

#[test]
fn wide_input_is_fine() {
    let wide: String = (0..50_000).map(|i| format!("k{i}: {i}\n")).collect();
    assert_eq!(parse(&wide).as_mapping().map(|m| m.len()), Some(50_000));
}

/// A flow collection on one line costs time proportional to that line, not to
/// the square of it.
///
/// `flow_key` and `plain_flow_scalar` used to call `line()`, which walks to
/// the newline — once per item, on a line each item then broke out of after a
/// few bytes. The cost of one item grew with the length of the line it sat on,
/// so a lockfile with one long `os: [...]` line was a denial of service on the
/// auditor rather than on anything it audits.
///
/// Two sizes on purpose. The small pair is what catches a regression: 71 ms
/// after the fix and 51.6 s before it, both debug on the machine this was
/// written on. The 5 s bound sits seventy times over the measurement and a
/// tenth of the way to the old cost, so a loaded box has room and a quadratic
/// scan still fails inside a minute. The megabyte is the shape the bug
/// arrived as — 471.8 s release before, 0.14 s after, 0.5 s in debug — and it
/// is only ever checked after the small pair, because at 1 MB a regression
/// takes hours to get around to failing.
#[test]
fn flow_collections_are_linear() {
    let seq = format!("os: [{}]\n", "a,".repeat(32_000));
    let map = format!(
        "m: {{{}}}\n",
        (0..8_000).map(|i| format!("k{i}: v, ")).collect::<String>()
    );
    let start = Instant::now();
    assert_eq!(
        parse(&seq)
            .get("os")
            .and_then(Value::as_sequence)
            .map(<[_]>::len),
        Some(32_000)
    );
    assert_eq!(
        parse(&map)
            .get("m")
            .and_then(Value::as_mapping)
            .map(|m| m.len()),
        Some(8_000)
    );
    let took = start.elapsed();
    assert!(
        took < Duration::from_secs(5),
        "96 KB of flow collections took {took:?}; the scanner is quadratic again"
    );

    let long_line = format!("os: [{}]\n", "a,".repeat(500_000));
    assert!(long_line.len() > 1_000_000);
    let start = Instant::now();
    assert_eq!(
        parse(&long_line)
            .get("os")
            .and_then(Value::as_sequence)
            .map(<[_]>::len),
        Some(500_000)
    );
    let took = start.elapsed();
    assert!(
        took < Duration::from_secs(20),
        "a 1 MB flow sequence took {took:?}"
    );
}

/// The same hostile line, unterminated. It has to come back as a positioned
/// error and not as a wall-clock problem — the quadratic scan was both.
#[test]
fn a_megabyte_on_one_line_errors_rather_than_hangs() {
    let start = Instant::now();
    assert_eq!(
        why(&format!("os: [{}", "a,".repeat(500_000))),
        "unclosed flow sequence"
    );
    assert_eq!(
        why(&format!(
            "m: {{{}",
            (0..200_000)
                .map(|i| format!("k{i}: v, "))
                .collect::<String>()
        )),
        "unclosed flow mapping"
    );
    // A megabyte of key with no `:` on it, which is the block-context version
    // of the same shape.
    assert_eq!(
        why(&"a".repeat(1024 * 1024)),
        "expected ':' after a mapping key"
    );
    // And truncated in the middle of the own-line flow form.
    assert_eq!(why("a:\n  {b: 1, c"), "unclosed flow mapping");
    let took = start.elapsed();
    assert!(
        took < Duration::from_secs(20),
        "refusing them took {took:?}"
    );
}
