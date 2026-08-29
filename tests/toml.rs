use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use stranger::error::Error;
use stranger::toml::{self, Value};

fn parse(s: &str) -> Value {
    toml::parse(s).unwrap_or_else(|e| panic!("{s:?} should parse: {e}"))
}

fn reject(s: &str) -> Error {
    match toml::parse(s) {
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

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn s(v: &str) -> Value {
    Value::String(v.into())
}

#[test]
fn scalars() {
    assert_eq!(parse("a = 4").get("a"), Some(&Value::Integer(4)));
    assert_eq!(parse("a = -0").get("a"), Some(&Value::Integer(0)));
    assert_eq!(parse("a = +7").get("a"), Some(&Value::Integer(7)));
    assert_eq!(
        parse("a = 1_000_000").get("a"),
        Some(&Value::Integer(1000000))
    );
    assert_eq!(parse("a = true").get("a"), Some(&Value::Bool(true)));
    assert_eq!(parse("a = false").get("a"), Some(&Value::Bool(false)));
    assert_eq!(parse(r#"a = "hi""#).get("a"), Some(&s("hi")));
    assert_eq!(parse("a = 'hi'").get("a"), Some(&s("hi")));
    assert_eq!(parse("").as_table().map(|t| t.len()), Some(0));
}

#[test]
fn integer_limits() {
    assert_eq!(
        parse("a = 9223372036854775807").get("a"),
        Some(&Value::Integer(i64::MAX))
    );
    assert_eq!(
        parse("a = -9223372036854775808").get("a"),
        Some(&Value::Integer(i64::MIN))
    );
    assert!(why("a = 9223372036854775808").contains("64 bits"));
    assert!(why("a = 01").contains("leading zeros"));
}

#[test]
fn escapes() {
    assert_eq!(parse(r#"a = "x\ny""#).get("a"), Some(&s("x\ny")));
    assert_eq!(
        parse(r#"a = "\b\f\r\t\\\"""#).get("a"),
        Some(&s("\u{8}\u{c}\r\t\\\""))
    );
    assert_eq!(parse(r#"a = "é""#).get("a"), Some(&s("é")));
    assert_eq!(parse(r#"a = "\U0001F980""#).get("a"), Some(&s("🦀")));
    // A literal string escapes nothing, which is the entire point of one.
    assert_eq!(parse(r#"a = 'x\ny'"#).get("a"), Some(&s(r"x\ny")));
}

/// TOML's escape list is not JSON's. `\/` is the one that would slip past a
/// parser written from JSON habits.
#[test]
fn bad_escapes() {
    reject(r#"a = "\/""#);
    reject(r#"a = "\x41""#);
    reject(r#"a = "\e""#);
    reject(r#"a = "\u00""#);
    reject(r#"a = "\u00zz""#);
    reject(r#"a = "\ud800""#);
    reject(r#"a = "\UFFFFFFFF""#);
}

#[test]
fn multiline_strings() {
    // The newline right after the opening delimiter is not content.
    assert_eq!(parse("a = \"\"\"\nx\ny\"\"\"").get("a"), Some(&s("x\ny")));
    assert_eq!(parse("a = '''\nx\ny'''").get("a"), Some(&s("x\ny")));
    assert_eq!(parse("a = \"\"\"\"\"\"").get("a"), Some(&s("")));
    // A backslash at end of line eats the newline and the indent after it.
    assert_eq!(
        parse("a = \"\"\"one \\\n     two\"\"\"").get("a"),
        Some(&s("one two"))
    );
    // Escapes are dead inside a triple-single-quoted string.
    assert_eq!(parse("a = '''x\\ny'''").get("a"), Some(&s("x\\ny")));
}

/// The closing delimiter is a run of three to five quotes, not three, because
/// the content is allowed to end in one or two of them.
#[test]
fn multiline_trailing_quotes() {
    assert_eq!(parse("a = \"\"\"x\"\"\"\"").get("a"), Some(&s("x\"")));
    assert_eq!(parse("a = \"\"\"x\"\"\"\"\"").get("a"), Some(&s("x\"\"")));
    assert_eq!(parse("a = \"\"\"x\"y\"\"\"").get("a"), Some(&s("x\"y")));
    reject("a = \"\"\"x\"\"\"\"\"\"");
}

#[test]
fn unterminated_strings() {
    reject(r#"a = "x"#);
    reject("a = 'x");
    reject("a = \"x\ny\"");
    reject("a = \"\"\"x");
    reject("a = '''x");
    reject("a = \"\\");
}

#[test]
fn tables() {
    let v = parse("[a]\nx = 1\n[a.b]\ny = 2\n");
    assert_eq!(
        v.get("a").and_then(|a| a.get("x")),
        Some(&Value::Integer(1))
    );
    assert_eq!(
        v.get("a").and_then(|a| a.get("b")).and_then(|b| b.get("y")),
        Some(&Value::Integer(2))
    );
    // `[a.b]` before `[a]` creates `a` implicitly; naming it later is legal.
    parse("[a.b]\nx = 1\n[a]\ny = 2\n");
}

#[test]
fn array_of_tables() {
    let v = parse("[[p]]\nname = \"one\"\n\n[[p]]\nname = \"two\"\n");
    let ps = v.get("p").and_then(Value::as_array).expect("p is an array");
    assert_eq!(ps.len(), 2);
    assert_eq!(ps[0].get("name"), Some(&s("one")));
    assert_eq!(ps[1].get("name"), Some(&s("two")));
}

/// The thing the whole module exists for: `[package.source]` after a
/// `[[package]]` attaches to that package, not to the array and not to a
/// fresh root table.
#[test]
fn subtable_binds_to_last_element() {
    let v = parse(
        "[[p]]\nname = \"one\"\n[p.src]\nurl = \"a\"\n\
         [[p]]\nname = \"two\"\n[p.src]\nurl = \"b\"\n",
    );
    let ps = v.get("p").and_then(Value::as_array).expect("p is an array");
    assert_eq!(ps[0].get("src").and_then(|x| x.get("url")), Some(&s("a")));
    assert_eq!(ps[1].get("src").and_then(|x| x.get("url")), Some(&s("b")));
}

/// poetry writes `"jaraco.classes" = "*"`. That is one key with a dot in it.
#[test]
fn quoted_key_with_a_dot() {
    let v = parse("[d]\n\"jaraco.classes\" = \"*\"\n");
    let d = v.get("d").and_then(Value::as_table).expect("d is a table");
    assert_eq!(d.get("jaraco.classes"), Some(&s("*")));
    assert_eq!(d.len(), 1);
    assert!(v.get("jaraco").is_none());
}

/// The key `"a.b"` and the path `a.b` are two namespaces TOML 1.0 keeps apart,
/// and the canonical key has to keep them apart too. Joining segments with `.`
/// did not: both spelled `a.b`, so this came back as
/// ``table `a.b` is defined twice`` — a poetry lockfile refused outright, and
/// a refused lockfile is a whole dependency tree nobody audited.
#[test]
fn a_quoted_dot_is_not_a_path() {
    let v = parse("\"a.b\" = 1\n[a.b]\nc = 2\n");
    assert_eq!(v.get("a.b"), Some(&Value::Integer(1)));
    assert_eq!(
        v.get("a").and_then(|a| a.get("b")).and_then(|b| b.get("c")),
        Some(&Value::Integer(2))
    );
    // The index suffix is the other half of the canonical form, and `[3]` was
    // spelled with characters a quoted key may also contain: `[[p]]` records
    // its first element as `p[0]`, which used to collide with a table named
    // exactly that.
    parse("[[p]]\nx = 1\n[\"p[0]\"]\ny = 2\n");
    // The index still does its job: one `[p.d]` per element is two tables.
    parse("[[p]]\n[p.d]\nx = 1\n[[p]]\n[p.d]\ny = 2\n");
}

#[test]
fn arrays() {
    assert_eq!(
        parse("a = []")
            .get("a")
            .map(|v| v.as_array().map(<[_]>::len)),
        Some(Some(0))
    );
    let v = parse("a = [\n  \"x\", # trailing junk\n  \"y\",\n]\n");
    assert_eq!(
        v.get("a").and_then(Value::as_array),
        Some(&[s("x"), s("y")][..])
    );
    let nested = parse("a = [[1], [2, 3]]");
    assert_eq!(
        nested.get("a").and_then(Value::as_array).map(<[_]>::len),
        Some(2)
    );
}

#[test]
fn inline_tables() {
    let v = parse(r#"a = { name = "x", extra = ["standard"], on = true }"#);
    let a = v.get("a").expect("a exists");
    assert_eq!(a.get("name"), Some(&s("x")));
    assert_eq!(a.get("on"), Some(&Value::Bool(true)));
    assert_eq!(
        a.get("extra").and_then(Value::as_array).map(<[_]>::len),
        Some(1)
    );
    assert_eq!(
        parse("a = {}")
            .get("a")
            .and_then(Value::as_table)
            .map(|t| t.len()),
        Some(0)
    );
    // poetry emits these with no padding at all.
    assert_eq!(
        parse(r#"a = {version = "*", markers = "os_name == \"nt\""}"#)
            .get("a")
            .and_then(|a| a.get("markers")),
        Some(&s(r#"os_name == "nt""#))
    );
}

#[test]
fn comments() {
    let v = parse("# lead\n\n a = 1 # after\n# tail\n");
    assert_eq!(v.get("a"), Some(&Value::Integer(1)));
    // A `#` inside a string is not a comment.
    assert_eq!(parse(r#"a = "x # y""#).get("a"), Some(&s("x # y")));
}

/// TOML 1.0 knows one carriage return, the one in CRLF: `newline = %x0A /
/// %x0D.0A`, and `non-eol`, the comment body, is `%x09 / %x20-7F / non-ascii`.
/// The comment scanner ran to the next `\n` regardless, so the first line here
/// parsed `Ok` as `{version = "1"}` — the `name` key silently gone, no error,
/// a package missing from the audit. That is the one failure a lockfile reader
/// may never produce, so the byte is refused where it stands.
#[test]
fn a_bare_carriage_return_is_refused() {
    let lost = "# c\rname = \"lodash\"\nversion = \"1\"\n";
    assert!(why(lost).contains("carriage return"), "{}", why(lost));
    assert_eq!(at(lost), (1, 4));
    // Trailing comment, same byte, the other scanner.
    assert!(why("a = 1 # c\rb = 2\n").contains("carriage return"));
    // And between statements, where it used to pass for whitespace.
    assert!(why("a = 1\n\rb = 2\n").contains("carriage return"));
    // CRLF still works everywhere it appears: after a comment, after a value,
    // and inside an array that spans lines.
    let v = parse("# c\r\nname = \"lodash\"\r\nn = [1,\r\n  2]\r\n");
    assert_eq!(v.get("name"), Some(&s("lodash")));
    assert_eq!(
        v.get("n").and_then(Value::as_array).map(<[_]>::len),
        Some(2)
    );
}

#[test]
fn refused_scalar_types() {
    assert!(why("a = 1.5").contains("floats"));
    assert!(why("a = 1e3").contains("floats"));
    assert!(why("a = 2026-03-26").contains("dates"));
    assert!(why("a = 07:32:00").contains("dates"));
    assert!(why("a = 1979-05-27T07:32:00Z").contains("dates"));
    assert!(why("a = 0xdeadbeef").contains("decimal"));
    assert!(why("a = 0o755").contains("decimal"));
    assert!(why("a = 0b1101").contains("decimal"));
    reject("a = inf");
    reject("a = nan");
}

/// `a = foo` used to report ``expected `false` `` — the parser dispatched on
/// the first byte, so a word starting with `f` was assumed to be a botched
/// `false`. Right position, invented expectation.
#[test]
fn value_errors_name_the_set() {
    let set = "expected a string, integer, boolean, array or inline table";
    assert_eq!(why("a = foo\n"), set);
    assert_eq!(why("a = tomorrow\n"), set);
    assert_eq!(at("a = foo\n"), (1, 5));
    // The keywords still parse, and a truncated one is still refused.
    assert_eq!(parse("a = true").get("a"), Some(&Value::Bool(true)));
    assert_eq!(parse("a = false").get("a"), Some(&Value::Bool(false)));
    assert_eq!(why("a = tru\n"), set);
    assert!(why("a = truely\n").contains("expected a newline"));
}

#[test]
fn refused_syntax() {
    assert!(why("a.b = 1").contains("dotted keys"));
    assert!(why("x = { a.b = 1 }").contains("dotted keys"));
    assert!(why("x = {\n  a = 1\n}").contains("one line"));
    assert!(why("[\"\"\"a\"\"\"]\nx = 1").contains("cannot be a key"));
}

#[test]
fn duplicate_keys() {
    assert!(why("a = 1\na = 2\n").contains("duplicate key `a`"));
    assert!(why("[t]\na = 1\na = 2\n").contains("duplicate key `a`"));
    assert!(why("x = { a = 1, a = 2 }").contains("duplicate key `a`"));
    // Two `[[p]]` blocks are two elements, not a duplicate.
    parse("[[p]]\na = 1\n[[p]]\na = 2\n");
    // But the same sub-table twice under one element is.
    assert!(why("[[p]]\n[p.d]\nx = 1\n[p.d]\ny = 2\n").contains("defined twice"));
    assert!(why("[t]\n[t]\n").contains("defined twice"));
}

#[test]
fn unclosed_and_mismatched() {
    reject("[a\nx = 1\n");
    reject("[[a]\nx = 1\n");
    reject("[a]]\nx = 1\n");
    reject("[]\n");
    assert!(why("a = [1, 2").contains("unclosed array"));
    assert!(why("a = { b = 1").contains("unclosed inline table"));
    reject("a = [1 2]");
    reject("a = { b = 1 c = 2 }");
    // An array-of-tables header cannot land on something that is not an array.
    assert!(why("[p]\nx = 1\n[[p]]\n").contains("already a table"));
}

/// What `=` defines is closed. `[[a]]` over a value array used to append a
/// table to it and hand back `[1, {b = 2}]`, which is not a value TOML can
/// represent; `package = []` followed by `[[package]]` read as a lockfile
/// with one package in it.
#[test]
fn a_header_cannot_reopen_what_an_equals_defined() {
    assert!(
        why("a = [1]\n[[a]]\nb = 2\n").contains("already defined as a value"),
        "{}",
        why("a = [1]\n[[a]]\nb = 2\n")
    );
    assert_eq!(at("a = [1]\n[[a]]\nb = 2\n"), (2, 1));
    assert!(
        why("package = []\n[[package]]\nname = \"x\"\n").contains("already defined as a value")
    );
    // An inline table is closed by its own brace, so a later header may not
    // extend it either.
    assert!(why("a = { b = 1 }\n[a]\nc = 2\n").contains("`a` is already defined as a value"));
    // Under an array element, where the canonical path carries an index.
    assert!(why("[[p]]\nd = [1]\n[[p.d]]\nx = 1\n").contains("`p[0].d` is already defined"));
    // And the legal shapes still are: two `[[p]]` are two elements, and a
    // key named after a *different* element's key is not a redefinition.
    parse("[[p]]\nd = [1]\n[[p]]\nd = [2]\n");
    parse("[[p]]\nname = \"one\"\n[p.src]\nurl = \"a\"\n");
}

/// A header one segment deeper than the closing brace is the same violation
/// wearing a longer path, and it used to be accepted: only the exact path was
/// tested against the sealed set, never a prefix of it. `[a.c]` therefore added
/// a key to a table TOML had closed, and the error names the brace's own path
/// rather than the header's, because that is the line you go and look at.
#[test]
fn a_header_cannot_reach_past_a_sealed_value() {
    assert!(why("a = {b = 1}\n[a.c]\nd = 2\n").contains("`a` is already defined as a value"));
    assert_eq!(at("a = {b = 1}\n[a.c]\nd = 2\n"), (2, 1));
    assert!(why("a = {b = 1}\n[a.c.d.e]\nf = 2\n").contains("`a` is already defined as a value"));
    // The shape a lockfile actually offers: uv writes `source = { … }` inline
    // under every `[[package]]`.
    assert!(
        why("[[p]]\nsrc = {kind = \"registry\"}\n[p.src.evil]\nrun = \"curl\"\n")
            .contains("`p[0].src` is already defined as a value")
    );
    // `[[a.c]]` reaches past it the same way.
    assert!(why("a = {b = 1}\n[[a.c]]\nd = 2\n").contains("`a` is already defined as a value"));
    // A sibling of a sealed key is not sealed: `[a.c]` is only wrong because
    // `a` was closed, not because anything named `c` was.
    parse("[a]\nb = {x = 1}\n[a.c]\nd = 2\n");
}

#[test]
fn junk_after_a_value() {
    reject("a = 1 2");
    reject("a = \"x\" \"y\"");
    reject("[a] junk\n");
    reject("a =");
    reject("a");
    reject("= 1");
}

/// The mark is skipped, and it is not a column. `error.rs` promises the
/// column lines up with what an editor shows, and counting three invisible
/// bytes as one character put every position on line 1 one to the right.
#[test]
fn leading_byte_order_mark() {
    assert_eq!(parse("\u{feff}a = 1\n").get("a"), Some(&Value::Integer(1)));
    assert_eq!(at("\u{feff}? = 1\n"), (1, 1));
    assert_eq!(at("\u{feff}a = 1.5\n"), (1, 6));
    assert_eq!(at("a = 1.5\n"), (1, 6));
}

#[test]
fn error_positions_point_at_the_problem() {
    // Line 3, column 5: the `.` that starts a float.
    assert_eq!(at("# c\nx = 1\ny = 1.5\n"), (3, 6));
    // The unterminated string is reported at its opening quote, not at EOF.
    assert_eq!(at("a = 1\nb = \"oops\n"), (2, 5));
    assert_eq!(at("a = [1,\n  2,\n"), (1, 5));
    // Column counts characters, so a multi-byte value does not skew it.
    assert_eq!(at("a = \"π\" junk\n"), (1, 9));
}

/// Cut a real file at every byte offset. Every prefix is either a valid
/// document or a positioned error; none of them may panic. One fixture per
/// format, because the three generators write different shapes and a cut
/// halfway through `{ url = "…", hash = "…"` is not the same bug as a cut
/// halfway through `[[package]]`.
///
// ponytail: capped at the first 4 KB of each file — truncation is quadratic,
// and the whole 29 KB `cargo-s` alone takes 50s in a debug build for coverage
// of the same constructs. Raise the cap if a crash ever turns up past it.
#[test]
fn truncation_never_panics() {
    for name in ["cargo-s.Cargo.lock", "poetry-s.poetry.lock", "uv-m.uv.lock"] {
        let full = fixture(name);
        for n in 0..full.len().min(4096) {
            if full.is_char_boundary(n) {
                let _ = toml::parse(&full[..n]);
            }
        }
    }
}

#[test]
fn deep_nesting_errors_rather_than_overflowing() {
    reject(&format!("a = {}", "[".repeat(10_000)));
    reject(&format!("a = {}", "{ b = ".repeat(10_000)));
}

/// This test used to assert the opposite — that a 500-segment header parses,
/// "because `descend` is iterative". It is, and that was never the question.
/// The nested `Value::Table` chain a header builds is *freed* recursively, so
/// `[a.b.b…]` 200,000 deep returned `Ok` and then took the process down with
/// `fatal runtime error: stack overflow` while dropping it. On a spawned
/// thread's 2 MiB stack, release: 30,001 segments survived, 35,001 aborted.
/// `scan_all` spawns exactly such a thread once a repo holds two lockfiles,
/// and `panic = "abort"` leaves nothing to catch.
#[test]
fn header_depth_is_bounded() {
    assert!(why(&format!("[a{}]\nx = 1\n", ".b".repeat(200_000))).contains("nesting deeper"));
    // 64 accepted, 65 refused, at the segment that went over.
    parse(&format!("[{}]\nx = 1\n", vec!["a"; 64].join(".")));
    assert!(why(&format!("[{}]\nx = 1\n", vec!["a"; 65].join("."))).contains("nesting deeper"));
    assert_eq!(
        at(&format!("[{}]\nx = 1\n", vec!["a"; 65].join("."))),
        (1, 130)
    );
    // The budget a header spends is handed back, not left on the counter: a
    // deep header followed by a nested value is legal and has to stay legal.
    parse(&format!("[{}]\nx = [[[1]]]\n", vec!["a"; 64].join(".")));
}

/// A megabyte on one line, in the shapes a hostile file can take it: a value,
/// a key, an array, a comment, and an unterminated string. Every one of them
/// has to come back — `Ok` or `Err` — without recursing or hanging. Five
/// megabytes of input, 0.48 s in a debug build.
#[test]
fn a_very_long_line_stays_linear() {
    let n = 1 << 20;
    assert_eq!(
        parse(&format!("a = \"{}\"", "x".repeat(n)))
            .get("a")
            .and_then(Value::as_str)
            .map(str::len),
        Some(n)
    );
    assert_eq!(
        parse(&format!("{} = 1", "a".repeat(n)))
            .as_table()
            .map(BTreeMap::len),
        Some(1)
    );
    assert_eq!(
        parse(&format!("a = [{}]", "1,".repeat(n / 2)))
            .get("a")
            .and_then(Value::as_array)
            .map(<[_]>::len),
        Some(n / 2)
    );
    parse(&format!("# {}\na = 1\n", "x".repeat(n)));
    assert!(why(&format!("a = \"{}", "x".repeat(n))).contains("unterminated"));
}

/// Package counts measured with `grep -c '^\[\[package\]\]'`, not copied from
/// anyone's notes.
const FIXTURES: &[(&str, usize)] = &[
    ("cargo-s.Cargo.lock", 124),
    ("cargo-m.Cargo.lock", 723),
    ("cargo-l.Cargo.lock", 944),
    ("poetry-s.poetry.lock", 54),
    ("poetry-m.poetry.lock", 233),
    ("uv-m.uv.lock", 250),
];

#[test]
fn every_fixture_parses() {
    for (name, expected) in FIXTURES {
        let src = fixture(name);
        let v = toml::parse(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let packages = v
            .get("package")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{name}: no [[package]] array"));
        assert_eq!(packages.len(), *expected, "{name}");
        for (i, p) in packages.iter().enumerate() {
            assert!(
                p.get("name").and_then(Value::as_str).is_some(),
                "{name}[{i}]"
            );
            assert!(
                p.get("version").and_then(Value::as_str).is_some(),
                "{name}[{i}]"
            );
        }
    }
}

#[test]
fn cargo_lock_version_is_an_integer() {
    for name in [
        "cargo-s.Cargo.lock",
        "cargo-m.Cargo.lock",
        "cargo-l.Cargo.lock",
    ] {
        let v = toml::parse(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            matches!(v.get("version"), Some(Value::Integer(3 | 4))),
            "{name}: {:?}",
            v.get("version")
        );
    }
}

/// poetry's `[package.dependencies]` is the sub-table binding, its inline
/// tables carry `\"` escapes, and `[metadata]` at the end is a root table
/// rather than another package sub-table. One package exercises all three.
#[test]
fn poetry_shape() {
    let v = toml::parse(&fixture("poetry-s.poetry.lock")).expect("poetry-s parses");
    let build = v
        .get("package")
        .and_then(Value::as_array)
        .and_then(|ps| ps.iter().find(|p| p.get("name") == Some(&s("build"))))
        .expect("poetry-s has a `build` package");
    assert_eq!(build.get("optional"), Some(&Value::Bool(false)));
    let colorama = build
        .get("dependencies")
        .and_then(|d| d.get("colorama"))
        .expect("build depends on colorama");
    assert_eq!(colorama.get("markers"), Some(&s(r#"os_name == "nt""#)));
    // files = [ {file = …, hash = …}, … ]
    let files = build
        .get("files")
        .and_then(Value::as_array)
        .expect("build has files");
    assert_eq!(files.len(), 2);
    assert!(
        files[0]
            .get("hash")
            .and_then(Value::as_str)
            .is_some_and(|h| h.starts_with("sha256:"))
    );

    assert_eq!(
        v.get("metadata").and_then(|m| m.get("lock-version")),
        Some(&s("2.1"))
    );
    assert!(v.get("extras").and_then(Value::as_table).is_some());
}

/// uv writes `source = { registry = … }` inline and keeps timestamps as
/// strings, which is the only reason a date-free subset is enough.
#[test]
fn uv_shape() {
    let v = toml::parse(&fixture("uv-m.uv.lock")).expect("uv-m parses");
    assert_eq!(v.get("version"), Some(&Value::Integer(1)));
    assert_eq!(v.get("revision"), Some(&Value::Integer(3)));
    let acp = v
        .get("package")
        .and_then(Value::as_array)
        .and_then(|ps| {
            ps.iter()
                .find(|p| p.get("name") == Some(&s("agent-client-protocol")))
        })
        .expect("uv-m has agent-client-protocol");
    assert_eq!(
        acp.get("source").and_then(|s| s.get("registry")),
        Some(&s("https://pypi.org/simple"))
    );
    assert!(
        acp.get("sdist")
            .and_then(|d| d.get("upload-time"))
            .and_then(Value::as_str)
            .is_some_and(|t| t.ends_with('Z'))
    );
    assert_eq!(
        acp.get("sdist").and_then(|d| d.get("size")),
        Some(&Value::Integer(71853))
    );
}
