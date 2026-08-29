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
    assert!(why("a = { b = 1 }\n[a]\nc = 2\n").contains("defined twice"));
    // Under an array element, where the canonical path carries an index.
    assert!(why("[[p]]\nd = [1]\n[[p.d]]\nx = 1\n").contains("already defined as a value"));
    // And the legal shapes still are: two `[[p]]` are two elements, and a
    // key named after a *different* element's key is not a redefinition.
    parse("[[p]]\nd = [1]\n[[p]]\nd = [2]\n");
    parse("[[p]]\nname = \"one\"\n[p.src]\nurl = \"a\"\n");
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
    // A long header path is iterative, not recursive, so it has to work.
    let path = vec!["a"; 500].join(".");
    parse(&format!("[{path}]\nx = 1\n"));
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
