//! RFC 8259, clause by clause, and then argued with a second implementation.
//!
//! `tests/json.rs` is the working test file: it covers what `src/json.rs` does
//! for the lockfiles this tool reads. This file is the conformance case. The
//! submission's claim is that a hand-written parser in one file makes
//! `serde_json` unnecessary for this job, and that claim rests on the grammar
//! rather than on a fixture passing.
//!
//! Two halves.
//!
//! **The suite.** One test per production of the grammar, each citing the
//! section it comes from — the six structural characters, the four whitespace
//! characters and the ones that only look like whitespace, the eight
//! two-character escapes (the RFC lists eight, not six: `"` `\` `/` `b` `f`
//! `n` `r` `t`), `\uXXXX` and every way a surrogate pair can go wrong, the
//! number grammar and each thing it forbids that `f64::from_str` would take,
//! the three literal names, duplicate keys, and the limits section 9 permits.
//!
//! **The campaign.** Two million generated documents and mutations of them,
//! fed to this parser and to CPython's `json`, comparing accept-or-reject and,
//! when both accept, the value. CPython is not RFC 8259 as it ships — it takes
//! `NaN` and `Infinity`, which section 6 does not have — so the oracle turns
//! those three back into errors with `parse_constant`, and every remaining
//! disagreement is then argued one at a time. There are 2,984 of them in four
//! classes, none of them about a value, and the argument for each is written
//! out on `differential_against_python` below. Publishing four classes and
//! saying who is right in each is worth more than publishing a zero.
//!
//! The campaign shells out to `python3`. That is dev-time tooling: it is not a
//! dependency, it is not in `Cargo.toml`, and it never runs in the binary. It
//! is `#[ignore]`d so that `cargo test` on a machine with no Python runs
//! everything else, and it prints a plain sentence rather than failing if
//! Python is missing.
//!
//!     ./scripts/json-differential.sh

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use stranger::json::{self, Value};

fn parse(s: &str) -> Value {
    json::parse(s).unwrap_or_else(|e| panic!("{s:?} should parse: {e}"))
}

fn reject(s: &str) {
    if let Ok(v) = json::parse(s) {
        panic!("{s:?} should not parse, got {v:?}");
    }
}

// ---------------------------------------------------------------- section 2

/// `JSON-text = ws value ws`. RFC 4627 allowed only an object or an array at
/// the top; 7159 widened it to any value and 8259 kept that.
#[test]
fn any_value_may_stand_alone() {
    assert_eq!(parse("null"), Value::Null);
    assert_eq!(parse("true"), Value::Bool(true));
    assert_eq!(parse("false"), Value::Bool(false));
    assert_eq!(parse("0"), Value::Number(0.0));
    assert_eq!(parse(r#""""#), Value::String(String::new()));
    assert_eq!(parse("[]"), Value::Array(vec![]));
    assert_eq!(parse("{}"), Value::Object(BTreeMap::new()));
}

/// Section 2: `ws` is exactly four characters. Everything else some language
/// calls whitespace — form feed, vertical tab, NBSP, the Unicode line and
/// paragraph separators, the ideographic space — is a character sitting where
/// a value has to be.
#[test]
fn whitespace_is_four_characters() {
    for ws in [" ", "\t", "\n", "\r", " \t\r\n "] {
        assert_eq!(parse(&format!("{ws}1{ws}")), Value::Number(1.0));
    }
    for not_ws in [
        "\u{b}", "\u{c}", "\u{a0}", "\u{2028}", "\u{2029}", "\u{3000}",
    ] {
        reject(&format!("{not_ws}1"));
        reject(&format!("[1,{not_ws}2]"));
    }
}

/// Section 2: six structural characters and no others. A parenthesis is not a
/// bracket and a semicolon is not a comma, which is the whole difference
/// between JSON and several of the configuration formats it gets mistaken for.
#[test]
fn six_structural_characters() {
    assert_eq!(parse("[1]").as_array().map(<[_]>::len), Some(1));
    assert!(parse(r#"{"a":1}"#).get("a").is_some());
    for s in ["(1)", "[1;2]", r#"{"a"=1}"#, "[1|2]", "<1>", "[1 2]"] {
        reject(s);
    }
}

/// Section 2, `ws value ws`, singular. Two values is not a JSON text, and
/// neither is none.
#[test]
fn one_value_and_no_more() {
    for s in [
        "", "   ", "\n", "1 2", "[1] [2]", "{} null", "nullnull", "[]x",
    ] {
        reject(s);
    }
}

// ---------------------------------------------------------------- section 3

/// Section 3: the three literal names are lowercase and whole. `True` is a
/// Python value, `NULL` is a C macro, `nil` is neither.
#[test]
fn literal_names_are_lowercase_and_whole() {
    assert_eq!(parse("true"), Value::Bool(true));
    assert_eq!(parse("false"), Value::Bool(false));
    assert_eq!(parse("null"), Value::Null);
    for s in [
        "True",
        "TRUE",
        "False",
        "NULL",
        "Null",
        "nil",
        "None",
        "undefined",
        "tru",
        "fals",
        "nul",
        "t",
        "f",
        "n",
    ] {
        reject(s);
    }
}

// ---------------------------------------------------------------- section 4

/// Section 4: `begin-object [ member *( value-separator member ) ] end-object`.
#[test]
fn objects_are_members_between_braces() {
    assert_eq!(parse("{}"), Value::Object(BTreeMap::new()));
    let two = parse(r#"{"a":1,"b":2}"#);
    assert_eq!(two.get("a"), Some(&Value::Number(1.0)));
    assert_eq!(two.get("b"), Some(&Value::Number(2.0)));
    // `ws` is allowed on both sides of the name separator and the value
    // separator, which is where a pretty-printer puts it.
    assert!(parse("{\n  \"a\" : 1 ,\n  \"b\" : 2\n}").get("b").is_some());
}

/// Section 4: `member = string name-separator value`. The name is a string —
/// not a bare word, not a number, and not a single-quoted anything.
#[test]
fn member_names_are_strings() {
    for s in [
        "{a:1}",
        "{1:2}",
        "{true:1}",
        "{null:1}",
        "{'a':1}",
        r#"{"a"}"#,
        r#"{"a",1}"#,
    ] {
        reject(s);
    }
}

#[test]
fn objects_reject_a_trailing_comma() {
    for s in [r#"{"a":1,}"#, "{,}", r#"{"a":1,,"b":2}"#, r#"{"a":1"b":2}"#] {
        reject(s);
    }
}

/// Section 4 says names SHOULD be unique and then declines to say what a
/// parser does when they are not. Last one wins here, which is what every
/// mainstream implementation does; this test is what pins it, because
/// "unspecified" is not the same as "whatever happens".
#[test]
fn duplicate_names_take_the_last() {
    assert_eq!(
        parse(r#"{"a":1,"a":2}"#).get("a"),
        Some(&Value::Number(2.0))
    );
    assert_eq!(
        parse(r#"{"a":1,"a":2}"#).as_object().map(BTreeMap::len),
        Some(1)
    );
    assert_eq!(parse(r#"{"a":[1],"a":null}"#).get("a"), Some(&Value::Null));
}

// ---------------------------------------------------------------- section 5

/// Section 5: `begin-array [ value *( value-separator value ) ] end-array`.
/// The elements need not be of the same type, which every schema language
/// bolted on top of JSON exists to constrain.
#[test]
fn arrays_are_values_between_brackets() {
    assert_eq!(parse("[]"), Value::Array(vec![]));
    assert_eq!(
        parse("[1, \"a\", true, null, [], {}]")
            .as_array()
            .map(<[_]>::len),
        Some(6)
    );
    assert_eq!(parse("[ 1 , 2 ]").as_array().map(<[_]>::len), Some(2));
}

#[test]
fn arrays_reject_a_trailing_or_missing_comma() {
    for s in ["[1,]", "[,]", "[,1]", "[1,,2]", "[1 2]", "[1"] {
        reject(s);
    }
}

// ---------------------------------------------------------------- section 6

/// Section 6: `int = zero / ( digit1-9 *DIGIT )`. A leading zero is not
/// permitted, which is the rule that stops `0755` from meaning two things.
#[test]
fn leading_zeros_are_rejected() {
    assert_eq!(parse("0"), Value::Number(0.0));
    assert_eq!(
        parse("-0").as_f64().map(f64::to_bits),
        Some((-0.0f64).to_bits())
    );
    for s in ["01", "00", "-01", "0123", "007"] {
        reject(s);
    }
}

/// Section 6: the only sign in `number` is `minus`, and only in front. A
/// leading plus is a thing `f64::from_str` takes and JSON does not.
#[test]
fn leading_plus_is_rejected() {
    for s in ["+1", "+0", "+1.5", "++1", "-+1"] {
        reject(s);
    }
}

/// Section 6: `frac = decimal-point 1*DIGIT`. At least one digit, on the right
/// of the point, and there is no production that puts one on the left of
/// nothing.
#[test]
fn the_decimal_point_needs_digits_on_both_sides() {
    assert_eq!(parse("0.5"), Value::Number(0.5));
    assert_eq!(parse("1.0"), Value::Number(1.0));
    for s in ["1.", ".5", "-.5", "1..2", ".", "1.2.3"] {
        reject(s);
    }
}

/// Section 6: `exp = e [ minus / plus ] 1*DIGIT`. Both cases of `e`, both
/// signs, and the digits are not optional.
#[test]
fn exponent_forms() {
    assert_eq!(parse("1e3"), Value::Number(1000.0));
    assert_eq!(parse("1E3"), Value::Number(1000.0));
    assert_eq!(parse("1e+3"), Value::Number(1000.0));
    assert_eq!(parse("1E-3"), Value::Number(0.001));
    assert_eq!(parse("1.5e2"), Value::Number(150.0));
    assert_eq!(parse("0e0"), Value::Number(0.0));
    for s in ["1e", "1e+", "1e-", "1e+ 3", "1ee3", "1e3.5", "e3", "1d3"] {
        reject(s);
    }
}

/// Section 6 forbids `Infinity` and `NaN` outright, and there is no hex or
/// octal production either. Every one of these is accepted by
/// `f64::from_str`, which is why `src/json.rs` walks the grammar by hand
/// before handing the slice over.
#[test]
fn numbers_rust_would_have_taken() {
    for s in [
        "inf",
        "-inf",
        "Infinity",
        "-Infinity",
        "NaN",
        "nan",
        "0x1F",
        "0b101",
        "1_000",
        "-",
        "1f32",
    ] {
        reject(s);
    }
}

/// Section 6 allows a parser to set limits on range and precision and says
/// what happens outside them is implementation-defined. Here it is f64: a
/// magnitude past 1e308 is infinity rather than an error, and seventeen
/// significant digits is where the exactness stops.
#[test]
fn range_and_precision_are_f64() {
    assert!(parse("1e309").as_f64().is_some_and(|n| n == f64::INFINITY));
    assert!(
        parse("-1e309")
            .as_f64()
            .is_some_and(|n| n == f64::NEG_INFINITY)
    );
    assert_eq!(parse("1e-400"), Value::Number(0.0));
    assert_eq!(parse("9007199254740993"), Value::Number(9007199254740992.0));
    // A number RFC 8259 permits and no f64 holds exactly. It is not an error,
    // it is a rounding, and nothing in a lockfile is a number this tool does
    // arithmetic on.
    assert_eq!(parse("0.1"), Value::Number(0.1));
}

// ---------------------------------------------------------------- section 7

/// Section 7 lists eight two-character escapes, not six: quotation mark,
/// reverse solidus, solidus, backspace, form feed, line feed, carriage return
/// and tab.
#[test]
fn the_eight_two_character_escapes() {
    assert_eq!(parse(r#""\"""#), Value::String("\"".into()));
    assert_eq!(parse(r#""\\""#), Value::String("\\".into()));
    assert_eq!(parse(r#""\/""#), Value::String("/".into()));
    assert_eq!(parse(r#""\b""#), Value::String("\u{8}".into()));
    assert_eq!(parse(r#""\f""#), Value::String("\u{c}".into()));
    assert_eq!(parse(r#""\n""#), Value::String("\n".into()));
    assert_eq!(parse(r#""\r""#), Value::String("\r".into()));
    assert_eq!(parse(r#""\t""#), Value::String("\t".into()));
}

/// Section 7: the escape list is closed. `\'` is a JavaScript habit, `\a` is
/// C, `\x41` is most things, and none of them are in the grammar.
#[test]
fn unknown_escapes_are_rejected() {
    for s in [
        r#""\a""#,
        r#""\'""#,
        r#""\x41""#,
        r#""\v""#,
        r#""\0""#,
        r#""\U0041""#,
        r#""\ ""#,
        r#""\""#,
    ] {
        reject(s);
    }
}

/// Section 7: the solidus MAY be escaped. It does not have to be, which is why
/// a URL in a lockfile parses either way and both spellings mean the same URL.
#[test]
fn the_solidus_is_optional_either_way() {
    assert_eq!(
        parse(r#""https://a/b""#),
        parse(r#""https:\/\/a\/b""#),
        "an escaped solidus is the same character as a bare one"
    );
}

/// Section 7: a string may not carry U+0000 through U+001F unescaped. All
/// thirty-two of them, not just the newline someone happened to test.
#[test]
fn unescaped_control_characters_are_rejected() {
    for c in 0u8..0x20 {
        reject(&format!("\"a{}b\"", c as char));
    }
    // U+007F is a control character in every table except this one: RFC 8259
    // stops at U+001F, so DEL goes through unescaped.
    assert_eq!(parse("\"a\u{7f}b\""), Value::String("a\u{7f}b".into()));
    // The one control character with a place in the grammar anyway: section 7
    // permits `\u0000`, so NUL is expressible, just never raw.
    assert_eq!(parse(r#""\u0000""#), Value::String("\u{0}".into()));
}

#[test]
fn strings_must_be_terminated() {
    for s in ["\"", "\"abc", r#""abc\""#, "[\"a]", "{\"a:1}"] {
        reject(s);
    }
}

/// Section 7: `\u` takes four hex digits, in either case, and nothing else.
#[test]
fn unicode_escape_takes_four_hex_digits() {
    assert_eq!(parse(r#""\u0041""#), Value::String("A".into()));
    assert_eq!(parse(r#""\u00e9""#), Value::String("é".into()));
    assert_eq!(parse(r#""\u00E9""#), Value::String("é".into()));
    assert_eq!(parse(r#""\uFFFD""#), Value::String("\u{fffd}".into()));
    for s in [
        r#""\u""#,
        r#""\u0""#,
        r#""\u00""#,
        r#""\u000""#,
        r#""\u00zz""#,
        r#""\u 041""#,
        r#""\u+041""#,
        r#""\u{41}""#,
    ] {
        reject(s);
    }
}

// ---------------------------------------------------------------- section 8

/// Section 8.1: a parser MAY ignore a byte-order mark at the start of a
/// stream. A lockfile written by a Windows editor carries one, and rejecting
/// it meant a valid `package-lock.json` audited zero packages.
#[test]
fn a_leading_byte_order_mark_is_skipped() {
    assert_eq!(parse("\u{feff}{}"), Value::Object(BTreeMap::new()));
    assert_eq!(parse("\u{feff}[1]").as_array().map(<[_]>::len), Some(1));
    // One, and only at the front. U+FEFF anywhere else is a character.
    reject("\u{feff}\u{feff}{}");
    reject("{\u{feff}}");
    reject("[1,\u{feff}2]");
    assert_eq!(parse("\"\u{feff}\""), Value::String("\u{feff}".into()));
}

/// Section 8.2 and the `\uXXXX` production in section 7: characters outside
/// the BMP arrive as a UTF-16 surrogate pair and have to be recombined.
#[test]
fn surrogate_pairs_recombine() {
    assert_eq!(parse(r#""\ud83e\udd80""#), Value::String("🦀".into()));
    assert_eq!(parse(r#""\uD83E\uDD80""#), Value::String("🦀".into()));
    // Both ends of the astral range.
    assert_eq!(
        parse(r#""\ud800\udc00""#),
        Value::String("\u{10000}".into())
    );
    assert_eq!(
        parse(r#""\udbff\udfff""#),
        Value::String("\u{10ffff}".into())
    );
    // The pair is one character, not two.
    assert_eq!(
        parse(r#""\ud83e\udd80""#)
            .as_str()
            .map(|s| s.chars().count()),
        Some(1)
    );
}

/// Section 8.2 calls a lone surrogate "not interoperable" rather than
/// forbidden, so a parser gets to choose. This one rejects, and the reason is
/// in the tool rather than in the RFC: the output of this parser feeds package
/// name comparison, and replacing a byte of a package name with U+FFFD is the
/// exact class of bug `stranger` exists to find. All five ways a surrogate can
/// arrive wrong.
#[test]
fn lone_and_mispaired_surrogates_are_rejected() {
    reject(r#""\ud83e""#); // high, then end of string
    reject(r#""\ud83eA""#); // high, then a character that is not an escape
    reject(r#""\ud83e\n""#); // high, then a different escape
    reject(r#""\ud83e\u0041""#); // high, then a non-surrogate escape
    reject(r#""\udd80""#); // low, with no high in front of it
    reject(r#""\udd80\ud83e""#); // the pair, backwards
    reject(r#""\ud83e\ud83e""#); // two highs
}

/// Section 8.1 says the text is UTF-8, and Rust's `&str` says so too. What is
/// worth pinning is that every scalar value survives the round trip,
/// noncharacters and the last code point included.
#[test]
fn every_scalar_value_survives() {
    // Written into the document literally rather than through `{:?}`, which
    // escapes a noncharacter as `\u{fffe}` — Rust's syntax, not JSON's, and
    // the first version of this test was checking the wrong parser.
    for s in [
        "a",
        "é",
        "€",
        "🦀",
        "\u{fffe}",
        "\u{ffff}",
        "\u{10ffff}",
        "\u{d7ff}",
        "\u{e000}",
    ] {
        let doc = format!("[\"{s}\"]");
        let parsed = parse(&doc);
        let got = parsed
            .as_array()
            .and_then(<[Value]>::first)
            .and_then(Value::as_str);
        assert_eq!(got, Some(s), "{doc:?}");
    }
}

// ---------------------------------------------------------------- section 9

/// Section 9: "An implementation may set limits on the maximum depth of
/// nesting." This one is 128, and the reason is that a recursive-descent
/// parser without a limit is a stack overflow with a hostile file's name on
/// it. 128 is an order of magnitude past the deepest real lockfile seen: the
/// 1,390-entry npm fixture nests 7.
#[test]
fn nesting_depth_is_capped_at_128() {
    let nest = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    parse(&nest(128));
    reject(&nest(129));
    reject(&nest(10_000));
    let objects = |n: usize| format!("{}1{}", r#"{"a":"#.repeat(n), "}".repeat(n));
    parse(&objects(128));
    reject(&objects(129));
}

/// Section 9 also permits a limit on the length of a text, and there is none
/// here beyond memory: width is not depth, and a lockfile is wide.
#[test]
fn width_is_not_capped() {
    let wide = format!("[{}]", vec!["1"; 100_000].join(","));
    assert_eq!(parse(&wide).as_array().map(<[_]>::len), Some(100_000));
}

// --------------------------------------------------- the differential campaign

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

/// Scalars that are awkward on purpose. A generator that emitted `{"a":1}`
/// would give the mutator nothing worth breaking, and the disagreements
/// between two JSON parsers are never in the object braces.
const SCALARS: [&str; 24] = [
    "null",
    "true",
    "false",
    "0",
    "-0",
    "1",
    "-1",
    "1.5",
    "-1.5e-2",
    "1E+3",
    "0.0001",
    "1e309",
    "9007199254740993",
    r#""""#,
    r#""a""#,
    r#""\u0000""#,
    r#""\ud83e\udd80""#,
    r#""\u00e9""#,
    r#""a\/b""#,
    r#""\t\n\r\b\f""#,
    r#""\\""#,
    r#""\"""#,
    r#""π""#,
    r#""🦀""#,
];

fn document(rng: &mut Rng, depth: u32) -> String {
    if depth == 0 || rng.next().is_multiple_of(3) {
        return (*rng.pick(&SCALARS)).to_string();
    }
    let n = rng.below(4);
    if rng.next().is_multiple_of(2) {
        let items: Vec<String> = (0..n).map(|_| document(rng, depth - 1)).collect();
        format!("[{}]", items.join(","))
    } else {
        let members: Vec<String> = (0..n)
            .map(|i| format!("\"k{i}\":{}", document(rng, depth - 1)))
            .collect();
        format!("{{{}}}", members.join(","))
    }
}

/// The characters worth putting into a JSON document to see what breaks. Hex
/// digits and `u` are in here because half the interesting rejections in this
/// grammar are inside a `\uXXXX`.
const ALPHABET: [char; 34] = [
    '{', '}', '[', ']', '"', ',', ':', '\\', '/', '-', '+', '.', 'e', 'E', 'u', 'd', 'D', '8', '0',
    '1', '9', 'a', 'f', 'F', 'n', 't', ' ', '\t', '\n', '\u{0}', '\u{1f}', '\u{7f}', '\u{feff}',
    '🦀',
];

fn mutate(base: &str, rng: &mut Rng) -> String {
    let mut chars: Vec<char> = base.chars().collect();
    for _ in 0..1 + rng.below(6) {
        if chars.is_empty() {
            chars.push(*rng.pick(&ALPHABET));
            continue;
        }
        let at = rng.below(chars.len());
        match rng.below(4) {
            0 => chars[at] = *rng.pick(&ALPHABET),
            1 => chars.insert(at, *rng.pick(&ALPHABET)),
            2 => {
                chars.remove(at);
            }
            _ => chars.truncate(at),
        }
    }
    chars.into_iter().collect()
}

/// The same canonical form `scripts/json-oracle.py` writes. Numbers go over as
/// IEEE-754 bits rather than as text because `repr(1e30)` is `1e+30` in Python
/// and `1e30` in Rust, and a differential harness that reports that as a
/// disagreement is reporting on itself.
fn canon_str(s: &str, out: &mut String) {
    out.push('s');
    for (i, c) in s.chars().enumerate() {
        if i > 0 {
            out.push('.');
        }
        write!(out, "{}", c as u32).expect("writing into a String cannot fail");
    }
}

fn canon(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push('n'),
        Value::Bool(true) => out.push('T'),
        Value::Bool(false) => out.push('F'),
        Value::Number(n) => {
            write!(out, "#{:016x}", n.to_bits()).expect("writing into a String cannot fail");
        }
        Value::String(s) => canon_str(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(item, out);
            }
            out.push(']');
        }
        // `BTreeMap<String, _>` iterates in UTF-8 byte order and Python's
        // `sorted` walks code points; those two orders agree for every string,
        // so neither side has to sort the other's way.
        Value::Object(map) => {
            out.push('{');
            for (i, (k, val)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon_str(k, out);
                out.push(':');
                canon(val, out);
            }
            out.push('}');
        }
    }
}

/// Documents chosen by hand, on top of the generated ones. Every entry here is
/// a place two JSON parsers are known to differ, or a place this one was
/// wrong once.
const HANDPICKED: [&str; 46] = [
    "",
    " ",
    "{}",
    "[]",
    "null",
    "1",
    "-0",
    "0e0",
    "1e309",
    "-1e309",
    "1e-400",
    "01",
    "1.",
    ".5",
    "+1",
    "1e",
    "0x1",
    "NaN",
    "Infinity",
    "-Infinity",
    "[1,]",
    "{\"a\":1,}",
    "{,}",
    "[1] [2]",
    "{\"a\" 1}",
    "{\"a\":1,\"a\":2}",
    "\u{feff}{}",
    "{\u{feff}}",
    "\"\\ud83e\\udd80\"",
    "\"\\ud83e\"",
    "\"\\udd80\"",
    "\"\\ud83eA\"",
    "\"\\ud83e\\u0041\"",
    "\"\\udd80\\ud83e\"",
    "\"\\u0000\"",
    "\"\\u007f\"",
    "\"\\/\"",
    "\"\\a\"",
    "\"a\u{7f}b\"",
    "\"a\u{1}b\"",
    "\"\u{2028}\"",
    "\t1\r\n",
    "\u{c}1",
    "\u{a0}1",
    "[[[[[[[[[[1]]]]]]]]]]",
    "{\"\":null}",
];

/// Two million because it costs 24 seconds. A tenth of it finds the same four
/// classes in the same proportions, which is the sign that the campaign has
/// stopped learning; `STRANGER_JSON_CAMPAIGN` moves it either way.
const CAMPAIGN: usize = 2_000_000;

fn campaign_size() -> usize {
    match std::env::var("STRANGER_JSON_CAMPAIGN") {
        Ok(s) => s.parse().expect("STRANGER_JSON_CAMPAIGN must be a usize"),
        Err(_) => CAMPAIGN,
    }
}

fn inputs(n: usize) -> Vec<String> {
    let mut rng = Rng::new(0x5DEECE66D);
    let mut out: Vec<String> = HANDPICKED.iter().map(|s| (*s).to_string()).collect();
    while out.len() < n {
        let doc = document(&mut rng, 4);
        // One clean document for every three broken ones. The clean ones are
        // what catch a disagreement about a *value*; the broken ones are what
        // catch a disagreement about the grammar, and only the first kind
        // would have caught a surrogate pair being recombined wrong.
        out.push(doc.clone());
        for _ in 0..3 {
            out.push(mutate(&doc, &mut rng));
        }
    }
    out.truncate(n);
    out
}

fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|exe| {
        Command::new(exe)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("json-differential");
    std::fs::create_dir_all(&dir).expect("target/ is writable during a test run");
    dir.join(name)
}

/// # What the campaign found
///
/// 2,000,000 inputs, 1,997,016 agreements, 2,984 disagreements in four
/// classes, and zero of them are a disagreement about a *value*: every time
/// both parsers accepted, they built the same thing, down to the IEEE-754
/// bits. The four classes, and who is right in each:
///
/// 1. **1,093 — a leading byte-order mark.** We skip it; CPython raises
///    "Unexpected UTF-8 BOM (decode using utf-8-sig)". Section 8.1 says an
///    implementation MAY ignore one, so both are inside the RFC and neither is
///    wrong. Ours is the more useful of the two permitted answers: a
///    `package-lock.json` saved by a Windows editor gets audited instead of
///    failing the whole file over three invisible bytes.
///
/// 2. **898 — a lone high surrogate**, `"\ud83e"`.
/// 3. **825 — a lone low surrogate**, `"\udd80"`.
/// 4. **168 — a high surrogate followed by a non-surrogate escape**,
///    `"\ud83e\u0041"`.
///
/// For those three, CPython builds a `str` holding an unpaired surrogate code
/// point and we reject. Section 8.2 does not forbid them — the ABNF admits
/// them — it calls strings containing them "not interoperable" and leaves the
/// behaviour to the implementation. So CPython is not violating the RFC, and
/// neither are we. Two reasons this one rejects. Rust's `String` cannot hold
/// an unpaired surrogate at all, so accepting means either WTF-8 or
/// substituting U+FFFD, and this parser's output feeds package-name
/// comparison: silently rewriting a character of a package name is the exact
/// class of bug `stranger` exists to find. And section 8.2's own word for
/// these strings is the argument — rejecting is the interoperable choice.
///
/// The one place CPython is plainly outside RFC 8259 does not appear in the
/// table, because the oracle switches it off: `json.loads("NaN")` is `nan`,
/// `json.loads("Infinity")` is `inf`, and section 6 has neither. Left on, that
/// would have been three more classes with CPython wrong in all three.
/// `parse_constant` turns them back into errors so that the campaign compares
/// this parser against the RFC rather than against Python's dialect of it.
#[test]
#[ignore = "shells out to python3; run with `./scripts/json-differential.sh`"]
fn differential_against_python() {
    let Some(python) = python() else {
        println!(
            "no python3 on this machine, so the differential campaign is skipped. \
             Everything else in this file ran. Install Python 3 and re-run \
             ./scripts/json-differential.sh to reproduce the published numbers."
        );
        return;
    };

    let docs = inputs(campaign_size());
    let frames = scratch("in.frames");
    let answers = scratch("out.lines");

    let mut buf = Vec::new();
    for doc in &docs {
        buf.extend_from_slice(format!("{}\n", doc.len()).as_bytes());
        buf.extend_from_slice(doc.as_bytes());
        buf.push(b'\n');
    }
    std::fs::write(&frames, &buf).expect("writing the campaign frames");

    let oracle = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("json-oracle.py");
    let status = Command::new(python)
        .arg(&oracle)
        .arg(&frames)
        .arg(&answers)
        .status()
        .expect("running the oracle");
    assert!(status.success(), "{} failed: {status}", oracle.display());

    let theirs = std::fs::read_to_string(&answers).expect("reading the oracle's answers");
    let theirs: Vec<&str> = theirs.lines().collect();
    assert_eq!(theirs.len(), docs.len(), "the oracle skipped a frame");

    // Bucketed by cause rather than listed. 200,000 inputs produce the same
    // handful of disagreements tens of thousands of times over, and 40,000
    // lines of output is not an argument — one line per cause, with the count
    // and one example, is the thing a reader can actually check.
    let mut buckets: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut agreed = 0usize;
    for (doc, theirs) in docs.iter().zip(theirs) {
        let ours = json::parse(doc);
        let mine = ours.as_ref().map(|v| {
            let mut s = String::new();
            canon(v, &mut s);
            s
        });
        let their_reason = theirs.strip_prefix("ERR").map(str::trim);
        let verdict = match (&mine, their_reason) {
            (Err(_), Some(_)) => {
                agreed += 1;
                continue;
            }
            (Ok(a), None) if theirs.strip_prefix("OK ") == Some(a.as_str()) => {
                agreed += 1;
                continue;
            }
            (Ok(_), Some(why)) => format!("ours accepts, python rejects ({why})"),
            (Err(_), None) => format!(
                "ours rejects, python accepts ({})",
                match &ours {
                    Err(stranger::error::Error::Syntax { what, .. }) => what.as_str(),
                    _ => "not a syntax error",
                }
            ),
            (Ok(_), None) => "both accept, values differ".to_string(),
        };
        let entry = buckets.entry(verdict).or_insert((0, String::new()));
        entry.0 += 1;
        if entry.1.is_empty() {
            entry.1 = format!(
                "{doc:?}  ours={}  python={theirs}",
                match &mine {
                    Ok(s) => format!("OK {s}"),
                    Err(e) => format!("ERR {e}"),
                }
            );
        }
    }

    let disagreements: usize = buckets.values().map(|(n, _)| n).sum();
    println!("campaign: {} inputs, {agreed} agreed", docs.len());
    println!(
        "disagreements: {disagreements} in {} classes",
        buckets.len()
    );
    for (verdict, (n, example)) in &buckets {
        println!("  {n:>7}  {verdict}\n           {example}");
    }
}
