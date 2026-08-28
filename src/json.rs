//! A JSON reader, RFC 8259.
//!
//! The interesting part is how position tracking works. The parser keeps the
//! original input next to the unconsumed remainder and asks the standard
//! library where one sits inside the other (`str::substr_range`, stable since
//! 1.98). So there is no cursor struct threaded through thirty functions and
//! no line counter to keep in sync — the cursor *is* the remainder, and the
//! offset is recoverable on demand. Line and column are only computed when an
//! error is actually being built, which is the one path where the cost of
//! rescanning the consumed prefix does not matter.

use crate::error::{Error, Result};
use std::collections::BTreeMap;

/// Deeper than this and we stop, rather than letting a hostile file walk the
/// parser off the end of the stack. Real lockfiles nest about ten deep; the
/// deepest thing in the 1,390-entry npm fixture is 7.
const MAX_DEPTH: u32 = 128;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// RFC 8259 puts no limit on the magnitude or precision of a number, and
    /// f64 does. Nothing in a lockfile is a number we do arithmetic on —
    /// versions and hashes are strings, `lockfileVersion` is small — so the
    /// lossy case is unreachable in practice and not worth a bignum.
    Number(f64),
    String(String),
    Array(Vec<Value>),
    /// Duplicate keys resolve last-one-wins, which is what every mainstream
    /// implementation does and what RFC 8259 declines to specify.
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }
}

pub fn parse(src: &str) -> Result<Value> {
    let mut p = Parser {
        src,
        rest: src,
        depth: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if !p.rest.is_empty() {
        return Err(p.err("trailing content after the top-level value"));
    }
    Ok(v)
}

struct Parser<'a> {
    src: &'a str,
    rest: &'a str,
    depth: u32,
}

impl<'a> Parser<'a> {
    fn err(&self, what: impl Into<String>) -> Error {
        let (line, col) = self.position();
        Error::Syntax {
            what: what.into(),
            line,
            col,
        }
    }

    /// Byte offset of the remainder inside the original input, converted to a
    /// 1-based line and character column. `substr_range` returns None only if
    /// `rest` is not derived from `src`, which cannot happen here.
    fn position(&self) -> (u32, u32) {
        let offset = self
            .src
            .substr_range(self.rest)
            .map_or(self.src.len(), |r| r.start);
        let consumed = &self.src[..offset];
        let line = consumed.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
        let col = match consumed.rfind('\n') {
            Some(nl) => consumed[nl + 1..].chars().count(),
            None => consumed.chars().count(),
        } as u32
            + 1;
        (line, col)
    }

    fn peek(&self) -> Option<u8> {
        self.rest.as_bytes().first().copied()
    }

    /// Only ever called with a count that lands on a UTF-8 boundary: either a
    /// byte we matched as ASCII, or the length of a char we just decoded.
    fn bump(&mut self, n: usize) {
        self.rest = &self.rest[n..];
    }

    fn skip_ws(&mut self) {
        let n = self
            .rest
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
            .count();
        self.bump(n);
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.peek() == Some(byte) {
            self.bump(1);
            Ok(())
        } else {
            Err(self.err(format!("expected '{}'", byte as char)))
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("expected a value")),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value> {
        if self.rest.starts_with(word) {
            self.bump(word.len());
            Ok(v)
        } else {
            Err(self.err(format!("expected `{word}`")))
        }
    }

    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err(format!("nesting deeper than {MAX_DEPTH}")));
        }
        Ok(())
    }

    fn array(&mut self) -> Result<Value> {
        self.enter()?;
        self.bump(1); // '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.bump(1);
            self.depth -= 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.bump(1),
                Some(b']') => {
                    self.bump(1);
                    break;
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Array(items))
    }

    fn object(&mut self) -> Result<Value> {
        self.enter()?;
        self.bump(1); // '{'
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.bump(1);
            self.depth -= 1;
            return Ok(Value::Object(map));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected a key"));
            }
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.bump(1),
                Some(b'}') => {
                    self.bump(1);
                    break;
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Object(map))
    }

    fn string(&mut self) -> Result<String> {
        self.bump(1); // opening quote
        let mut out = String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.err("unterminated string"));
            };
            match b {
                b'"' => {
                    self.bump(1);
                    return Ok(out);
                }
                b'\\' => {
                    self.bump(1);
                    self.escape(&mut out)?;
                }
                // RFC 8259 section 7: unescaped control characters are not
                // allowed in a string. Rejecting them is also the cheapest
                // way to notice you have been handed a truncated or binary file.
                0x00..=0x1F => return Err(self.err("unescaped control character in string")),
                _ => {
                    let c = self
                        .rest
                        .chars()
                        .next()
                        .expect("peek said there was a byte");
                    self.bump(c.len_utf8());
                    out.push(c);
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<()> {
        let Some(b) = self.peek() else {
            return Err(self.err("unterminated escape"));
        };
        let simple = match b {
            b'"' => Some('"'),
            b'\\' => Some('\\'),
            b'/' => Some('/'),
            b'b' => Some('\u{8}'),
            b'f' => Some('\u{c}'),
            b'n' => Some('\n'),
            b'r' => Some('\r'),
            b't' => Some('\t'),
            _ => None,
        };
        if let Some(c) = simple {
            self.bump(1);
            out.push(c);
            return Ok(());
        }
        if b != b'u' {
            return Err(self.err(format!("unknown escape `\\{}`", b as char)));
        }
        self.bump(1);
        out.push(self.unicode_escape()?);
        Ok(())
    }

    /// `\uXXXX`, including the surrogate pair dance.
    ///
    /// JSON strings are sequences of UTF-16 code units, so anything outside the
    /// BMP arrives as a high surrogate followed by a low one and has to be
    /// recombined. A surrogate that arrives alone is rejected rather than
    /// replaced with U+FFFD: this parser's output feeds package-name
    /// comparison, and silently rewriting a byte of a package name is exactly
    /// the class of bug this tool exists to find.
    fn unicode_escape(&mut self) -> Result<char> {
        let first = self.hex4()?;
        let scalar = match first {
            0xD800..=0xDBFF => {
                if !self.rest.starts_with("\\u") {
                    return Err(self.err("high surrogate not followed by a low surrogate"));
                }
                self.bump(2);
                let second = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&second) {
                    return Err(self.err("high surrogate followed by a non-surrogate"));
                }
                0x10000 + ((first as u32 - 0xD800) << 10) + (second as u32 - 0xDC00)
            }
            0xDC00..=0xDFFF => return Err(self.err("low surrogate with no high surrogate")),
            _ => first as u32,
        };
        char::from_u32(scalar).ok_or_else(|| self.err("escape is not a Unicode scalar value"))
    }

    fn hex4(&mut self) -> Result<u16> {
        let digits = self
            .rest
            .get(..4)
            .ok_or_else(|| self.err("truncated \\u escape"))?;
        let mut n: u16 = 0;
        for b in digits.bytes() {
            let d = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(self.err("\\u escape needs four hex digits")),
            };
            n = n * 16 + d as u16;
        }
        self.bump(4);
        Ok(n)
    }

    /// Rust's `f64::from_str` is more permissive than JSON: it takes `inf`,
    /// `NaN`, `1.`, `.5` and `+1`. So the grammar from RFC 8259 section 6 is
    /// checked here first, and only the validated slice is handed over.
    fn number(&mut self) -> Result<Value> {
        let start = self.rest;
        let mut n = 0usize;
        let bytes = start.as_bytes();

        if bytes.first() == Some(&b'-') {
            n += 1;
        }
        match bytes.get(n) {
            Some(b'0') => n += 1,
            Some(b'1'..=b'9') => {
                while matches!(bytes.get(n), Some(b'0'..=b'9')) {
                    n += 1;
                }
            }
            _ => return Err(self.err("expected a digit")),
        }
        if bytes.get(n) == Some(&b'.') {
            n += 1;
            if !matches!(bytes.get(n), Some(b'0'..=b'9')) {
                return Err(self.err("expected a digit after '.'"));
            }
            while matches!(bytes.get(n), Some(b'0'..=b'9')) {
                n += 1;
            }
        }
        if matches!(bytes.get(n), Some(b'e' | b'E')) {
            n += 1;
            if matches!(bytes.get(n), Some(b'+' | b'-')) {
                n += 1;
            }
            if !matches!(bytes.get(n), Some(b'0'..=b'9')) {
                return Err(self.err("expected a digit in the exponent"));
            }
            while matches!(bytes.get(n), Some(b'0'..=b'9')) {
                n += 1;
            }
        }

        let text = &start[..n];
        self.bump(n);
        // Unreachable for any slice the scan above accepts; the only inputs
        // f64 rejects are ones that never get here.
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| self.err("number out of range"))
    }
}
