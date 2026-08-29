use stranger::error::Error;
use stranger::json::{self, Value};

fn parse(s: &str) -> Value {
    json::parse(s).unwrap_or_else(|e| panic!("{s:?} should parse: {e}"))
}

fn reject(s: &str) -> Error {
    match json::parse(s) {
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

#[test]
fn scalars() {
    assert_eq!(parse("null"), Value::Null);
    assert_eq!(parse("true"), Value::Bool(true));
    assert_eq!(parse("false"), Value::Bool(false));
    assert_eq!(parse("  1  "), Value::Number(1.0));
    assert_eq!(parse(r#""hi""#), Value::String("hi".into()));
}

#[test]
fn numbers() {
    assert_eq!(parse("-0"), Value::Number(-0.0));
    assert_eq!(parse("1e3"), Value::Number(1000.0));
    assert_eq!(parse("1E+3"), Value::Number(1000.0));
    assert_eq!(parse("-1.5e-2"), Value::Number(-0.015));
    assert_eq!(parse("0.5"), Value::Number(0.5));
}

/// Everything here is accepted by `f64::from_str` and rejected by RFC 8259,
/// which is the whole reason the grammar is checked by hand before parsing.
#[test]
fn numbers_rust_would_have_taken() {
    for s in [
        "01", "1.", ".5", "+1", "inf", "NaN", "0x1", "1e", "1e+", "-",
    ] {
        reject(s);
    }
}

#[test]
fn escapes() {
    assert_eq!(parse(r#""a\nb""#), Value::String("a\nb".into()));
    assert_eq!(parse(r#""A""#), Value::String("A".into()));
    assert_eq!(parse(r#""\/\\\"""#), Value::String("/\\\"".into()));
    assert_eq!(
        parse(r#""\b\f\r\t""#),
        Value::String("\u{8}\u{c}\r\t".into())
    );
}

/// U+1F980 arrives as the pair D83E DD80 and has to be recombined.
#[test]
fn surrogate_pair() {
    assert_eq!(parse(r#""🦀""#), Value::String("🦀".into()));
}

#[test]
fn lone_surrogates_are_rejected() {
    // A high surrogate with nothing after it, with a non-surrogate after it,
    // and a low surrogate standing on its own.
    reject(r#""\ud83e""#);
    reject(r#""\ud83eA""#);
    reject(r#""\udd80""#);
}

#[test]
fn bad_escapes() {
    reject(r#""\x41""#);
    reject(r#""\u00""#);
    reject(r#""\u00zz""#);
    reject(r#""\""#);
}

#[test]
fn control_characters_must_be_escaped() {
    reject("\"a\nb\"");
    reject("\"a\u{0}b\"");
}

#[test]
fn containers() {
    assert_eq!(parse("[]"), Value::Array(vec![]));
    assert_eq!(parse("{}"), Value::Object(Default::default()));
    assert_eq!(
        parse("[1,2]"),
        Value::Array(vec![Value::Number(1.0), Value::Number(2.0)])
    );
    let o = parse(r#"{"a": {"b": [true]}}"#);
    assert_eq!(
        o.get("a")
            .and_then(|v| v.get("b"))
            .and_then(|v| v.as_array())
            .map(<[_]>::len),
        Some(1)
    );
}

#[test]
fn trailing_commas_and_junk() {
    reject("[1,]");
    reject("{\"a\":1,}");
    reject("{,}");
    reject("[1] [2]");
    reject("{\"a\" 1}");
    reject("");
    reject("   ");
}

/// Last one wins, which is what every other implementation does.
#[test]
fn duplicate_keys() {
    assert_eq!(
        parse(r#"{"a":1,"a":2}"#).get("a"),
        Some(&Value::Number(2.0))
    );
}

#[test]
fn error_positions_point_at_the_problem() {
    // Line 2, character 10: the `}` sitting where a value should be.
    assert_eq!(at("{\n  \"key\": }"), (2, 10));
    assert_eq!(at("[1, 2, x]"), (1, 8));
    // Column counts characters, not bytes, so a multi-byte name does not skew it.
    assert_eq!(at(r#"{"π": }"#), (1, 7));
}

/// RFC 8259 section 8.1 lets a parser skip a leading byte-order mark, and a
/// lockfile saved by a Windows editor carries one. Rejecting it failed the
/// whole file — exit 2, no findings — over three invisible bytes.
#[test]
fn leading_byte_order_mark() {
    assert_eq!(parse("\u{feff}{}"), Value::Object(Default::default()));
    assert_eq!(parse("\u{feff}[1]"), Value::Array(vec![Value::Number(1.0)]));
    assert_eq!(parse("\u{feff}\"x\""), Value::String("x".into()));
    // Skipped, not counted: the column is the one an editor shows.
    assert_eq!(at("\u{feff}{\"a\": }"), (1, 7));
    // One mark, and only at the front. Anywhere else it is not whitespace.
    reject("\u{feff}\u{feff}{}");
    reject("{\u{feff}}");
}

#[test]
fn truncation_never_panics() {
    let full = r#"{"packages":{"node_modules/a":{"version":"1.0.0","dev":true}}}"#;
    for n in 0..full.len() {
        let _ = json::parse(&full[..n]);
    }
}

/// Ten thousand open brackets is a stack overflow in a naive recursive
/// descent parser. It has to come back as an error instead.
#[test]
fn deep_nesting_errors_rather_than_overflowing() {
    let deep = "[".repeat(10_000);
    reject(&deep);
    let deep_objects = "{\"a\":".repeat(10_000);
    reject(&deep_objects);
}

#[test]
fn wide_input_is_fine() {
    let wide = format!("[{}]", vec!["1"; 100_000].join(","));
    assert_eq!(parse(&wide).as_array().map(<[_]>::len), Some(100_000));
}
