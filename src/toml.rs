//! A TOML reader, cut down to what lockfiles actually contain.
//!
//! Position tracking works the same way as `json.rs`: the unconsumed
//! remainder *is* the cursor, and `str::substr_range` recovers the byte
//! offset when an error needs a line and column. The one addition here is
//! `err_at`, which takes a remembered remainder, so an unterminated string
//! can be reported at its opening quote rather than at end of file.
//!
//! # The subset
//!
//! Accepted:
//!
//! - `key = value` at the top level and inside `[table]` blocks
//! - `[table]` and `[dotted.table]` headers
//! - `[[array.of.tables]]` — `[[package]]`, and the whole reason this exists
//! - basic strings `"…"` with `\b \t \n \f \r \" \\ \uXXXX \UXXXXXXXX`
//! - literal strings `'…'`
//! - multi-line `"""…"""` and `'''…'''`, including the line-ending backslash
//! - decimal integers, with `_` separators and an optional sign
//! - `true` / `false`
//! - arrays, over as many lines as they like, trailing comma allowed
//! - inline tables `{ a = 1, b = 2 }`, one line each, no trailing comma
//! - `#` comments to end of line
//!
//! Refused, with a position:
//!
//! - floats, dates, times, and date-times as bare values
//! - hex, octal and binary integers
//! - dotted keys (`a.b = 1`) outside a table header
//! - inline tables spread over several lines (that is TOML 1.1)
//! - duplicate keys, and any header that reopens something already defined:
//!   a `[table]` written twice, a `[[a]]` over an `a = […]`, an `[a]` — or an
//!   `[a.c]`, which reaches past it — over an `a = { … }`
//!
//! # What the fixtures actually contain
//!
//! Measured, not assumed, across six real lockfiles (three `Cargo.lock`,
//! two `poetry.lock`, one `uv.lock`):
//!
//! - No triple-quoted string appears anywhere. Not one. Multi-line strings
//!   are implemented because a lockfile is allowed to contain them and
//!   mis-reading one would be worse than refusing it, but no generator in
//!   this corpus emits them.
//! - No literal `'…'` string appears at a value position either. Every
//!   single quote in the corpus is *inside* a basic string —
//!   `marker = "sys_platform == 'win32'"` in `uv.lock`, and the environment
//!   markers poetry writes.
//! - The only escape any generator emits is `\"`, and only poetry emits it,
//!   in nested markers like `markers = "os_name == \"nt\""`.
//! - poetry writes quoted keys with dots in them: `"jaraco.classes" = "*"`.
//!   That is a single key whose name contains a dot, *not* a dotted key, and
//!   conflating the two would silently invent a `jaraco` table. Quoting is
//!   therefore what decides, not the dot.
//! - Every inline table in the corpus fits on one line, which is what
//!   TOML 1.0 requires anyway.
//! - The only bare integers are `version` and `revision` at the top of the
//!   file: 1, 3 and 4. No floats, no dates. `uv.lock` does record timestamps
//!   — `upload-time = "2026-03-26T01:21:00.379Z"` — but as strings.
//! - `[package.dependencies]`, `[package.extras]`, `[package.source]`,
//!   `[package.metadata]` and `[package.optional-dependencies]` all attach
//!   to the last `[[package]]` pushed, which is the state the array-of-tables
//!   handling exists to keep.

use crate::error::{Error, Result};
use std::collections::{BTreeMap, HashSet};

/// Arrays, inline tables and table headers all count against this. Lockfiles
/// nest three deep at the most (`wheels = [ { … } ]` is two), so it is only
/// here to stop a hostile file walking the parser off the stack.
///
/// Headers were exempt at first, on the reasoning that `descend` is a loop and
/// a loop cannot overflow. That is true of *building* the tree and false of
/// freeing it. `[a.b.b…]` 200,000 segments long parses clean and returns `Ok`
/// in 333 ms; the nested `Value::Table` chain it hands back is then dropped
/// recursively, one frame per segment, and that is where the process dies —
/// `fatal runtime error: stack overflow`, which `panic = "abort"` makes
/// uncatchable. On a spawned thread's 2 MiB stack a release build survived
/// 30,001 segments and aborted on 35,001, so 70 KB of input is enough; a debug
/// build gave out between 2,501 and 2,601, at 5 KB. The parser was never the
/// thing that died.
const MAX_DEPTH: u32 = 64;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    /// TOML integers are specified as 64-bit signed, so this is the whole
    /// range and not a lossy stand-in. Anything wider is an error.
    Integer(i64),
    Bool(bool),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Table(map) => map.get(key),
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

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Table(map) => Some(map),
            _ => None,
        }
    }
}

/// The document is always a table, so the return is always `Value::Table`.
pub fn parse(src: &str) -> Result<Value> {
    // `src` is the body, not the original: positions are computed against it,
    // and a mark left in front of them puts every column on line 1 one to the
    // right of where an editor shows it. `json.rs` and `yaml.rs` agree.
    let body = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut p = Parser {
        src: body,
        rest: body,
        depth: 0,
    };

    let mut root: BTreeMap<String, Value> = BTreeMap::new();
    // Header paths that have been opened with an explicit `[header]`, in a
    // form that distinguishes `package[3].source` from `package[4].source`.
    // Without the index every `[package.dependencies]` after the first would
    // look like a redefinition.
    let mut defined: HashSet<Vec<Seg>> = HashSet::new();
    // The same paths, for keys defined with `=`. TOML calls what a `=`
    // defines an immutable namespace, and the distinction is not pedantry:
    // `a = [1]` is an array of *values*, which `[[a]]` may not push onto, and
    // `a = {b = 1}` is closed by its own brace, which `[a]` may not reopen.
    // Nothing in the tree records which of the two built a table, so it has
    // to be recorded here.
    let mut immutable: HashSet<Vec<Seg>> = HashSet::new();
    let mut path: Vec<String> = Vec::new();
    let mut canon: Vec<Seg> = Vec::new();

    loop {
        p.skip_blank();
        if p.rest.is_empty() {
            break;
        }
        if p.peek() == Some(b'[') {
            path = p.header(&mut root, &mut defined, &immutable)?;
            continue;
        }

        let key_at = p.rest;
        let key = p.simple_key()?;
        p.skip_spaces();
        if p.peek() == Some(b'.') {
            return Err(p.err("dotted keys are not part of the supported TOML subset"));
        }
        p.expect(b'=')?;
        p.skip_spaces();
        let value = p.value()?;
        p.end_of_line()?;

        canon.clear();
        let table = descend(&mut root, &path, &mut canon)
            .ok_or_else(|| p.err_at(key_at, "this key sits under a non-table"))?;
        if table.insert(key.clone(), value).is_some() {
            return Err(p.err_at(key_at, format!("duplicate key `{key}`")));
        }
        canon.push(Seg::Key(key));
        immutable.insert(canon.clone());
    }

    Ok(Value::Table(root))
}

/// One step of a canonical path: a key, or an index into an array of tables.
///
/// This was a `String` with the steps joined by `.` and an index written
/// `[3]`, a spelling that cannot represent a key containing either character.
/// `"a.b" = 1` and `[a.b]` are two different namespaces in TOML 1.0 and both
/// flattened to `a.b`, so poetry's own `"jaraco.classes" = "*"` sitting beside
/// a `[jaraco]` table came back as ``table `a.b` is defined twice``. A refused
/// lockfile is a whole dependency tree left unaudited, which is worse than any
/// finding this tool could have reported about it.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Seg {
    Key(String),
    Index(usize),
}

/// A canonical path spelled the way the file spelled it, for an error message.
fn show(path: &[Seg]) -> String {
    let mut out = String::new();
    for (i, seg) in path.iter().enumerate() {
        match seg {
            Seg::Key(k) => {
                // Indexed rather than `out.is_empty()`, because an empty
                // first segment — `[""."x"]` is legal, if unhinged — leaves
                // `out` empty and would cost the *next* segment its dot.
                if i > 0 {
                    out.push('.');
                }
                out.push_str(k);
            }
            Seg::Index(n) => {
                out.push('[');
                out.push_str(&n.to_string());
                out.push(']');
            }
        }
    }
    out
}

/// The shortest prefix of `path` that a `=` already sealed, if there is one.
///
/// Testing the whole path and nothing else let `a = {b = 1}` be extended by
/// `[a.c]`: the set holds `a`, the header asks about `a.c`, and the exact
/// answer is no. So any header *deeper* than the closing brace could add keys
/// to a table TOML had sealed. The shape a lockfile offers: uv writes
/// `source = { registry = "…" }` under every `[[package]]`, and
/// `[package.source.evil]` walked straight into it.
fn sealed_prefix<'p>(immutable: &HashSet<Vec<Seg>>, path: &'p [Seg]) -> Option<&'p [Seg]> {
    (1..=path.len())
        .map(|n| &path[..n])
        .find(|prefix| immutable.contains(*prefix))
}

/// Walk `path` from the root, creating tables that do not exist yet, and
/// record the walk in `canon` so the caller can tell two same-named headers
/// under different array elements apart.
///
/// The array arm is the array-of-tables rule: `[package.source]` after
/// `[[package]]` belongs to the package that was pushed last, not to the
/// array. Returns `None` when a segment collides with something that is
/// neither a table nor an array of tables.
fn descend<'t>(
    root: &'t mut BTreeMap<String, Value>,
    path: &[String],
    canon: &mut Vec<Seg>,
) -> Option<&'t mut BTreeMap<String, Value>> {
    let mut cur = root;
    for seg in path {
        canon.push(Seg::Key(seg.clone()));
        let slot = cur
            .entry(seg.clone())
            .or_insert_with(|| Value::Table(BTreeMap::new()));
        cur = match slot {
            Value::Table(t) => t,
            Value::Array(items) => {
                canon.push(Seg::Index(items.len().saturating_sub(1)));
                match items.last_mut() {
                    Some(Value::Table(t)) => t,
                    _ => return None,
                }
            }
            _ => return None,
        };
    }
    Some(cur)
}

struct Parser<'a> {
    src: &'a str,
    rest: &'a str,
    depth: u32,
}

impl<'a> Parser<'a> {
    fn err(&self, what: impl Into<String>) -> Error {
        self.err_at(self.rest, what)
    }

    /// Same as `err`, but positioned at a remainder we saved earlier —
    /// "unterminated string" is only useful if it points at the quote that
    /// opened it, not at the end of the file.
    fn err_at(&self, at: &str, what: impl Into<String>) -> Error {
        let (line, col) = self.position_of(at);
        Error::Syntax {
            what: what.into(),
            line,
            col,
        }
    }

    fn position_of(&self, at: &str) -> (u32, u32) {
        let offset = self
            .src
            .substr_range(at)
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
    /// run of bytes matched as ASCII, or the length of a char just decoded.
    fn bump(&mut self, n: usize) {
        self.rest = &self.rest[n..];
    }

    /// Horizontal whitespace only. Newlines are structure in TOML, so the
    /// two kinds of skipping have to stay separate — that is what makes
    /// `a = 1 2` and a multi-line inline table detectable at all.
    fn skip_spaces(&mut self) {
        let n = self
            .rest
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        self.bump(n);
    }

    /// Whitespace, newlines and whole comment lines. Used between statements
    /// and inside arrays, both of which may span lines.
    fn skip_blank(&mut self) {
        loop {
            let n = self
                .rest
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                .count();
            self.bump(n);
            if self.peek() != Some(b'#') {
                return;
            }
            let n = self.rest.bytes().take_while(|&b| b != b'\n').count();
            self.bump(n);
        }
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.peek() == Some(byte) {
            self.bump(1);
            Ok(())
        } else {
            Err(self.err(format!("expected '{}'", byte as char)))
        }
    }

    /// A statement owns the rest of its line. Nothing may follow a value
    /// except spaces and a comment.
    fn end_of_line(&mut self) -> Result<()> {
        self.skip_spaces();
        if self.peek() == Some(b'#') {
            let n = self.rest.bytes().take_while(|&b| b != b'\n').count();
            self.bump(n);
        }
        match self.peek() {
            None => Ok(()),
            Some(b'\n') => {
                self.bump(1);
                Ok(())
            }
            Some(b'\r') if self.rest.starts_with("\r\n") => {
                self.bump(2);
                Ok(())
            }
            Some(_) => Err(self.err("expected a newline")),
        }
    }

    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err(format!("nesting deeper than {MAX_DEPTH}")));
        }
        Ok(())
    }

    /// `[a.b]` or `[[a.b]]`. Returns the path that subsequent key/value lines
    /// are written into.
    fn header(
        &mut self,
        root: &mut BTreeMap<String, Value>,
        defined: &mut HashSet<Vec<Seg>>,
        immutable: &HashSet<Vec<Seg>>,
    ) -> Result<Vec<String>> {
        let at = self.rest;
        self.bump(1);
        let array = self.peek() == Some(b'[');
        if array {
            self.bump(1);
        }

        // A header is a statement, not a nested value, so the budget it spends
        // is handed straight back rather than unwound one `enter` at a time.
        // Leaking it would make a legal file — a deep header followed by an
        // array — fail for having been preceded by something long.
        let outer = self.depth;
        let mut path = Vec::new();
        loop {
            self.skip_spaces();
            self.enter()?;
            path.push(self.simple_key()?);
            self.skip_spaces();
            if self.peek() == Some(b'.') {
                self.bump(1);
                continue;
            }
            break;
        }
        self.depth = outer;
        self.expect(b']')?;
        if array {
            self.expect(b']')?;
        }
        self.end_of_line()?;

        let mut canon = Vec::new();
        if array {
            // Every `[[x]]` pushes a fresh table, so the parent path is what
            // gets resolved and the last segment is the array itself.
            let (last, parents) = path
                .split_last()
                .expect("the loop above pushes at least one key");
            let parent = descend(root, parents, &mut canon)
                .ok_or_else(|| self.err_at(at, "this header sits under a non-table"))?;
            canon.push(Seg::Key(last.clone()));
            // `a = [1]` then `[[a]]` used to append a table to the value
            // array, producing `[1, {…}]` — a mixed array TOML has no way to
            // write down, and, for `package = []`, a lockfile that reads as
            // one package here and zero everywhere else. The `let … else`
            // below only catches `a` having been a table.
            if let Some(sealed) = sealed_prefix(immutable, &canon) {
                return Err(self.err_at(
                    at,
                    format!(
                        "`{}` is already defined as a value, not an array of tables",
                        show(sealed)
                    ),
                ));
            }
            let slot = parent
                .entry(last.clone())
                .or_insert_with(|| Value::Array(Vec::new()));
            let Value::Array(items) = slot else {
                return Err(self.err_at(at, format!("`{last}` is already a table, not an array")));
            };
            items.push(Value::Table(BTreeMap::new()));
            canon.push(Seg::Index(items.len() - 1));
            defined.insert(canon);
        } else {
            descend(root, &path, &mut canon)
                .ok_or_else(|| self.err_at(at, "this header sits under a non-table"))?;
            // An inline table reaches `descend` as an ordinary table, and the
            // tree keeps no record of which of `=` and `[…]` built it, so the
            // second set is the only thing that stops `a = {b = 1}` growing a
            // key TOML says cannot be added.
            if let Some(sealed) = sealed_prefix(immutable, &canon) {
                return Err(self.err_at(
                    at,
                    format!(
                        "`{}` is already defined as a value, not a table",
                        show(sealed)
                    ),
                ));
            }
            if !defined.insert(canon) {
                let name = path.join(".");
                return Err(self.err_at(at, format!("table `{name}` is defined twice")));
            }
        }

        Ok(path)
    }

    /// One key: bare, or quoted. Deliberately *not* a dotted key — the caller
    /// decides whether a following `.` continues a path or is an error.
    fn simple_key(&mut self) -> Result<String> {
        if self.rest.starts_with("\"\"\"") || self.rest.starts_with("'''") {
            return Err(self.err("a multi-line string cannot be a key"));
        }
        match self.peek() {
            Some(b'"') => self.basic_string(),
            Some(b'\'') => self.literal_string(),
            Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-') => {
                let n = self
                    .rest
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                    .count();
                let key = self.rest[..n].to_string();
                self.bump(n);
                Ok(key)
            }
            _ => Err(self.err("expected a key")),
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'"' | b'\'') => Ok(Value::String(self.string()?)),
            Some(b'[') => self.array(),
            Some(b'{') => self.inline_table(),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'+' | b'-' | b'0'..=b'9') => self.integer(),
            Some(_) => Err(self.err("expected a value")),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn word(&mut self, word: &str, v: Value) -> Result<Value> {
        if self.rest.starts_with(word) {
            self.bump(word.len());
            Ok(v)
        } else {
            Err(self.err(format!("expected `{word}`")))
        }
    }

    fn array(&mut self) -> Result<Value> {
        self.enter()?;
        let open = self.rest;
        self.bump(1);
        let mut items = Vec::new();
        loop {
            // Checking for the close before every element is what makes both
            // `[]` and a trailing comma fall out for free.
            self.skip_blank();
            match self.peek() {
                Some(b']') => {
                    self.bump(1);
                    break;
                }
                None => return Err(self.err_at(open, "unclosed array")),
                _ => {}
            }
            items.push(self.value()?);
            self.skip_blank();
            match self.peek() {
                Some(b',') => self.bump(1),
                Some(b']') => {
                    self.bump(1);
                    break;
                }
                None => return Err(self.err_at(open, "unclosed array")),
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Array(items))
    }

    fn inline_table(&mut self) -> Result<Value> {
        self.enter()?;
        let open = self.rest;
        self.bump(1);
        let mut map = BTreeMap::new();
        self.skip_spaces();
        if self.peek() == Some(b'}') {
            self.bump(1);
            self.depth -= 1;
            return Ok(Value::Table(map));
        }
        loop {
            self.skip_spaces();
            // TOML 1.0 keeps an inline table on one line. Saying so beats
            // swallowing the rest of the file looking for a '}'.
            if matches!(self.peek(), Some(b'\n' | b'\r')) {
                return Err(self.err("an inline table must fit on one line"));
            }
            let key_at = self.rest;
            let key = self.simple_key()?;
            self.skip_spaces();
            if self.peek() == Some(b'.') {
                return Err(self.err("dotted keys are not part of the supported TOML subset"));
            }
            self.expect(b'=')?;
            self.skip_spaces();
            let v = self.value()?;
            if map.insert(key.clone(), v).is_some() {
                return Err(self.err_at(key_at, format!("duplicate key `{key}`")));
            }
            self.skip_spaces();
            match self.peek() {
                Some(b',') => self.bump(1),
                Some(b'}') => {
                    self.bump(1);
                    break;
                }
                Some(b'\n' | b'\r') => {
                    return Err(self.err("an inline table must fit on one line"));
                }
                None => return Err(self.err_at(open, "unclosed inline table")),
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Table(map))
    }

    fn string(&mut self) -> Result<String> {
        if self.rest.starts_with("\"\"\"") {
            self.multiline(b'"')
        } else if self.rest.starts_with("'''") {
            self.multiline(b'\'')
        } else if self.peek() == Some(b'\'') {
            self.literal_string()
        } else {
            self.basic_string()
        }
    }

    fn basic_string(&mut self) -> Result<String> {
        let open = self.rest;
        self.bump(1);
        let mut out = String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.err_at(open, "unterminated string"));
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
                b'\n' | b'\r' => return Err(self.err_at(open, "unterminated string")),
                0x00..=0x08 | 0x0b..=0x1f | 0x7f => {
                    return Err(self.err("control character in string"));
                }
                _ => self.push_char(&mut out),
            }
        }
    }

    fn literal_string(&mut self) -> Result<String> {
        let open = self.rest;
        self.bump(1);
        let mut out = String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.err_at(open, "unterminated literal string"));
            };
            match b {
                b'\'' => {
                    self.bump(1);
                    return Ok(out);
                }
                b'\n' | b'\r' => return Err(self.err_at(open, "unterminated literal string")),
                0x00..=0x08 | 0x0b..=0x1f | 0x7f => {
                    return Err(self.err("control character in string"));
                }
                _ => self.push_char(&mut out),
            }
        }
    }

    /// `"""…"""` and `'''…'''` differ only in the delimiter and in whether
    /// backslashes mean anything, so they share a body.
    ///
    /// Two details bite here. First, the delimiter is not "three quotes" but
    /// "a run of three to five quotes", because TOML lets the content end in
    /// up to two of them, so `"""a""""` is `a"` and not a syntax error.
    /// Counting the whole run and handing back `run - 3` of them is the only
    /// way to avoid eating the closing quotes. Second, a newline immediately
    /// after the opening delimiter is not content.
    fn multiline(&mut self, quote: u8) -> Result<String> {
        let open = self.rest;
        self.bump(3);
        if self.rest.starts_with("\r\n") {
            self.bump(2);
        } else if self.peek() == Some(b'\n') {
            self.bump(1);
        }

        let escapes = quote == b'"';
        let mut out = String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.err_at(open, "unterminated multi-line string"));
            };
            if b == quote {
                let run = self.rest.bytes().take_while(|&c| c == quote).count();
                if run >= 6 {
                    return Err(self.err("too many quotes to close a multi-line string"));
                }
                if run >= 3 {
                    for _ in 0..run - 3 {
                        out.push(quote as char);
                    }
                    self.bump(run);
                    return Ok(out);
                }
                for _ in 0..run {
                    out.push(quote as char);
                }
                self.bump(run);
                continue;
            }
            match b {
                b'\\' if escapes => {
                    // A backslash whose line has nothing left on it swallows
                    // the newline and all the indentation after it. Anything
                    // else is an ordinary escape.
                    let line = self.rest;
                    let after = &line[1..];
                    let ws = after
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    let folds = matches!(after.as_bytes().get(ws), Some(b'\n'))
                        || after[ws..].starts_with("\r\n");
                    self.bump(1);
                    if folds {
                        let n = self
                            .rest
                            .bytes()
                            .take_while(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                            .count();
                        self.bump(n);
                    } else {
                        self.escape(&mut out)?;
                    }
                }
                // Tab, newline and carriage return are content here; the rest
                // of the C0 range still is not.
                0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f => {
                    return Err(self.err("control character in string"));
                }
                _ => self.push_char(&mut out),
            }
        }
    }

    fn push_char(&mut self, out: &mut String) {
        let c = self
            .rest
            .chars()
            .next()
            .expect("the caller peeked a byte, so there is a char");
        self.bump(c.len_utf8());
        out.push(c);
    }

    /// TOML's escape list is JSON's minus `\/`, plus `\U` for the astral
    /// planes. Rejecting `\/` matters more than it looks: it is the one
    /// escape a JSON-shaped assumption would let through.
    fn escape(&mut self, out: &mut String) -> Result<()> {
        let Some(b) = self.peek() else {
            return Err(self.err("unterminated escape"));
        };
        let simple = match b {
            b'"' => Some('"'),
            b'\\' => Some('\\'),
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
        let width = match b {
            b'u' => 4,
            b'U' => 8,
            _ => return Err(self.err(format!("unknown escape `\\{}`", b as char))),
        };
        self.bump(1);
        let c = self.hex(width)?;
        out.push(c);
        Ok(())
    }

    fn hex(&mut self, width: usize) -> Result<char> {
        let digits = self
            .rest
            .get(..width)
            .ok_or_else(|| self.err("truncated unicode escape"))?;
        let mut n: u32 = 0;
        for b in digits.bytes() {
            let d = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(self.err(format!("unicode escape needs {width} hex digits"))),
            };
            n = n * 16 + d as u32;
        }
        self.bump(width);
        // Also the surrogate check: D800-DFFF are not scalar values, and
        // substituting U+FFFD for one would quietly rewrite a package name.
        char::from_u32(n).ok_or_else(|| self.err("escape is not a Unicode scalar value"))
    }

    /// Decimal integers only, and the refusals are the point.
    ///
    /// A float or a date starts exactly like an integer, so a parser that
    /// scans digits and stops leaves `1.5` looking like `1` with junk after
    /// it, and `2026-03-26` looking like `2026`. Both get caught here, at the
    /// character that gives the game away, rather than turning into a
    /// confusing "expected a newline" three columns later.
    fn integer(&mut self) -> Result<Value> {
        let start = self.rest;
        let bytes = start.as_bytes();
        let mut n = 0;
        let negative = bytes.first() == Some(&b'-');
        if matches!(bytes.first(), Some(b'+' | b'-')) {
            n += 1;
        }
        if bytes.get(n) == Some(&b'0')
            && matches!(
                bytes.get(n + 1),
                Some(b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
            )
        {
            return Err(self.err_at(&start[n..], "only decimal integers are supported"));
        }

        let mut digits = String::new();
        loop {
            match bytes.get(n) {
                Some(d @ b'0'..=b'9') => {
                    digits.push(*d as char);
                    n += 1;
                }
                // `_` is only a separator between digits; a leading, trailing
                // or doubled one falls out of the loop and trips end_of_line.
                Some(b'_')
                    if !digits.is_empty() && matches!(bytes.get(n + 1), Some(b'0'..=b'9')) =>
                {
                    n += 1;
                }
                _ => break,
            }
        }
        if digits.is_empty() {
            return Err(self.err_at(&start[n..], "expected a digit"));
        }

        // Before the leading-zero rule, because `07:32:00` is a time and
        // "an integer may not have leading zeros" would be a lie about it.
        match bytes.get(n) {
            Some(b'.' | b'e' | b'E') => {
                return Err(self.err_at(
                    &start[n..],
                    "floats are not part of the supported TOML subset",
                ));
            }
            Some(b'-' | b':') => {
                return Err(self.err_at(
                    &start[n..],
                    "dates and times are not part of the supported TOML subset",
                ));
            }
            _ => {}
        }
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(self.err_at(start, "an integer may not have leading zeros"));
        }

        let mut text = String::with_capacity(digits.len() + 1);
        if negative {
            text.push('-');
        }
        text.push_str(&digits);
        self.bump(n);
        text.parse::<i64>()
            .map(Value::Integer)
            .map_err(|_| self.err_at(start, "integer does not fit in 64 bits"))
    }
}
