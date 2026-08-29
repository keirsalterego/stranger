//! A YAML reader, cut down to what `pnpm-lock.yaml` actually contains.
//!
//! Position tracking works the same way as `json.rs` and `toml.rs`: the
//! unconsumed remainder *is* the cursor, and `str::substr_range` recovers the
//! byte offset when an error needs a line and column. `err_at` takes a
//! remembered remainder so an unclosed `{` is reported at the brace that
//! opened it rather than at the end of the line.
//!
//! Full YAML is one of the largest data formats in circulation — anchors,
//! aliases, tags, multiple documents, block scalars with indentation
//! indicators, folding rules, merge keys, and an implicit type system that
//! turns `no` into `false`. None of that is here. The design rule is the one
//! `toml.rs` uses: **anything outside the subset is refused with a line and a
//! column, never guessed at.** A parser that improvises produces a plausible
//! wrong answer, and a plausible wrong answer about which package is in a
//! lockfile is worse than no answer.
//!
//! # The subset
//!
//! Accepted:
//!
//! - block mappings, nested by indentation, `key: value` and bare `key:`
//! - block sequences, `- item`, indented *under* their key
//! - plain scalars, single-quoted `'…'` (with `''` for a literal quote), and
//!   double-quoted `"…"` with the JSON escape set plus `\0`
//! - flow mappings `{a: 1, b: 2}` and flow sequences `[a, b]`, on one line,
//!   each with an optional trailing comma
//! - a flow collection on the line *under* its key, indented past it —
//!   `resolution:` then `{integrity: …}`. pnpm writes the inline spelling;
//!   any reformatter may write this one, and the two produce the same node
//! - `#` comments: a whole line, or after a value when preceded by a space
//! - blank lines anywhere, a leading byte-order mark, and one `---` opening
//!   the document
//!
//! Refused, with a position:
//!
//! - a tab anywhere in a line's indentation (YAML forbids it, and it is the
//!   one whitespace bug an editor will not show you)
//! - a dedent that lands between two open levels
//! - anchors `&a`, aliases `*a`, tags `!!str`, directives `%YAML`, block
//!   scalars `|` and `>` — refused *by name*, at the indicator, in all four
//!   places unquoted text is read, flow mapping keys included. The plain
//!   scalar scanner would otherwise read `&anchor 1` as the string
//!   "&anchor 1" and be wrong without saying so
//! - a plain scalar or key opening with a flow indicator `{`, `}`, `[`, `]`
//!   or `,`. YAML forbids that too, and it is what keeps `{a: 1}: 2` — a flow
//!   mapping used as a key, which this subset does not have — from being read
//!   as the key `{a`
//! - multi-line quoted scalars, and everything else about documents: a
//!   second `---`, a `...` end marker, and content on a `---` line
//! - a flow collection spread over more than one line
//! - a block sequence at the same indentation as its key (`key:\n- a`)
//! - a mapping inside a sequence item (`- a: 1`)
//! - a duplicate key in the same mapping
//!
//! # The bomb that is not here
//!
//! Billion laughs — a dozen anchors, each aliasing the one before it,
//! expanding to gigabytes out of a few hundred bytes — is closed, and not by
//! a limit. There is no alias resolution machinery in this parser at all.
//! `&a` and `*a` are refused at the indicator wherever unquoted text is read,
//! so nothing is ever recorded to expand and nothing exists to expand it.
//! `MAX_DEPTH` guards the stack against nesting; it is not what stops this.
//!
//! # Implicit typing: `true` and `false`, and nothing else
//!
//! YAML's implicit typing is the format's most famous footgun. YAML 1.1 reads
//! `no`, `NO`, `off` and `n` as `false` — the "Norway problem", named for the
//! country code that stops being a string when you write it down. It reads
//! `1.0` as a float, so a version that round-trips through a typing parser
//! comes back as `1`. It reads `08` as a string but `010` as octal 8.
//!
//! This parser types exactly two tokens: lowercase `true` and lowercase
//! `false`. Everything else stays a `String`, including `9.0`, `null`, `~`,
//! `Yes`, `1e3` and `0x10`. That is not laziness, it is the only choice that
//! is safe here — `no`, `on`, `y` and `off` are all real package names on the
//! npm registry, and a reader that turned the key `no@1.0.0` into a boolean
//! would drop a package from a supply-chain audit without saying a word. The
//! two booleans are typed only because the fixture needs them: `hasBin`,
//! `optional`, `autoInstallPeers` and `excludeLinksFromLockfile` are all
//! read as booleans by `lock::pnpm`.
//!
//! `Value::Null` is produced by exactly one thing: a key whose value is
//! empty and which opens no nested block. The *text* `null` is a string.
//!
//! # What the fixture actually contains
//!
//! Measured across `fixtures/pnpm-l.pnpm-lock.yaml`, 7,782 lines,
//! lockfileVersion 9.0, the only YAML in the corpus:
//!
//! - Zero tabs, zero comments, zero double-quoted scalars, zero block
//!   scalars, zero anchors or aliases, zero document markers, one document.
//! - 1,705 blank lines — one between every package entry — so blank-line
//!   handling is not an edge case, it is the common path. A blank line
//!   belongs to no indentation level and must not close a block.
//! - 1,570 flow mappings: every `resolution: {integrity: …}` and every
//!   `engines: {node: …}`. 128 flow sequences, all `os:` and `cpu:`.
//! - 95 block sequence items, all under `transitivePeerDependencies:`, all
//!   indented two columns past their key.
//! - 444 single-quoted keys. pnpm quotes a key when it starts with `@` —
//!   which is a reserved indicator in YAML — and leaves the rest plain, so
//!   `zwitch@2.0.4:` and `'@babel/core@7.27.1':` sit side by side. Keys also
//!   contain `/`, `(`, `)` and `.`; the key `.` is the root importer.
//! - Values contain `:` inside quotes (`deprecated: 'SECURITY: …'`) and
//!   never outside them. That is what lets a plain scalar refuse an
//!   unquoted `: ` outright instead of guessing where the key ends.
//!
//! Double-quoted scalars are implemented despite appearing nowhere, on the
//! same reasoning `toml.rs` gives for multi-line strings: a lockfile is
//! allowed to contain one, and refusing a legal file is a false negative on
//! a whole dependency tree.

use crate::error::{Error, Result};
use std::collections::BTreeMap;

/// Block nesting in a pnpm lockfile is five levels at the deepest
/// (`packages` → entry → `peerDependenciesMeta` → name → `optional`). Flow
/// collections recurse too. This is only here to stop a hostile file walking
/// the parser off the stack.
const MAX_DEPTH: u32 = 128;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A key with an empty value and no block under it. Never the text `null`.
    Null,
    Bool(bool),
    String(String),
    Sequence(Vec<Value>),
    /// Duplicate keys are an error rather than last-one-wins. YAML 1.2 says
    /// they are invalid, and a second `integrity:` quietly shadowing the
    /// first is the shape of bug this tool exists to notice.
    Mapping(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Mapping(map) => map.get(key),
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

    pub fn as_sequence(&self) -> Option<&[Value]> {
        match self {
            Value::Sequence(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_mapping(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Mapping(map) => Some(map),
            _ => None,
        }
    }
}

pub fn parse(src: &str) -> Result<Value> {
    // The body is what positions are measured against; a mark left in front of
    // them shifts every column on line 1. `json.rs` and `toml.rs` agree.
    let body = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut p = Parser {
        src: body,
        rest: body,
        depth: 0,
        started: false,
    };

    let Some(indent) = p.next_content()? else {
        // Nothing but blanks and comments. YAML calls that the empty
        // document; the lockfile reader has a better message for it than a
        // syntax error would.
        return Ok(Value::Null);
    };
    let doc = p.block(indent)?;
    // The only way to get here with input left is a line indented *less* than
    // the document's own first line, which cannot happen when that line is at
    // column 1 — but a file that starts indented can do it.
    if let Some(n) = p.next_content()? {
        let at = &p.rest[n..];
        return Err(p.err_at(at, "this line is outdented past the start of the document"));
    }
    Ok(doc)
}

struct Parser<'a> {
    src: &'a str,
    rest: &'a str,
    depth: u32,
    /// Set once the document has begun — by its first content line, or by a
    /// leading `---`. A marker after that opens a second document.
    started: bool,
}

impl<'a> Parser<'a> {
    fn err(&self, what: impl Into<String>) -> Error {
        self.err_at(self.rest, what)
    }

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

    /// Only ever called with a count that lands on a UTF-8 boundary: a run of
    /// bytes matched as ASCII, or the length of a char just decoded.
    fn bump(&mut self, n: usize) {
        self.rest = &self.rest[n..];
    }

    /// The rest of the current line, not including its terminator.
    fn line(&self) -> &'a str {
        let rest = self.rest;
        let n = rest
            .bytes()
            .take_while(|&b| b != b'\n' && b != b'\r')
            .count();
        &rest[..n]
    }

    /// Spaces and tabs. A tab is legal *separation* whitespace in YAML —
    /// only indentation forbids it — so this is not where tabs are caught.
    fn skip_inline(&mut self) {
        let n = self
            .rest
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        self.bump(n);
    }

    fn skip_line(&mut self) {
        let n = self.rest.bytes().take_while(|&b| b != b'\n').count();
        self.bump((n + 1).min(self.rest.len()));
    }

    /// Advance to the next line that carries content and report how far it is
    /// indented, leaving `rest` at the *start* of that line so the caller can
    /// decide whether the line belongs to it before committing to it.
    ///
    /// This is where every block-structure decision is made, so two rules
    /// live here and nowhere else.
    ///
    /// First, blank lines and comment-only lines belong to no indentation
    /// level at all. They can neither open, close, nor continue a block. That
    /// is not a nicety: the fixture puts a blank line between all 850 package
    /// entries, so a parser that let a zero-width line close the `packages:`
    /// block would read exactly one package and report a clean project.
    ///
    /// Second, indentation is spaces. A tab is refused here, at the tab, with
    /// a column — YAML forbids tabs in indentation outright, and it is the one
    /// whitespace error an editor renders as if it were correct.
    fn next_content(&mut self) -> Result<Option<usize>> {
        loop {
            if self.rest.is_empty() {
                return Ok(None);
            }
            let n = self.rest.bytes().take_while(|&b| b == b' ').count();
            let after = &self.rest[n..];
            if after.starts_with('\t') {
                return Err(self.err_at(after, "tab used for indentation"));
            }
            // A marker only counts at column 1, which is what leaves an
            // indented `---` alone as the plain scalar it is.
            if n == 0
                && let Some(marker) = document_marker(after)
            {
                self.document(marker)?;
                continue;
            }
            match after.as_bytes().first() {
                // Trailing spaces at end of file.
                None => {
                    self.rest = after;
                    return Ok(None);
                }
                Some(b'\n' | b'\r' | b'#') => self.skip_line(),
                _ => {
                    self.started = true;
                    return Ok(Some(n));
                }
            }
        }
    }

    /// A `---` or `...` at column 1, with `rest` sitting on it.
    ///
    /// The choice here is to accept one leading `---` and refuse everything
    /// else about documents. Accepting it is the cheap half: a YAML dumper is
    /// free to write one, pnpm's own output does not but a hand-edited or
    /// re-serialised lockfile can, and refusing a legal file is a false
    /// negative on a whole dependency tree. Refusing the rest is the half that
    /// matters — left alone, `plain_key` reads `--- a: 1` as the key "--- a"
    /// and a second document's packages either overwrite the first's or
    /// collide as duplicates, and both of those are wrong answers rather than
    /// no answer.
    fn document(&mut self, marker: &str) -> Result<()> {
        if marker == "..." {
            return Err(self.err("a document end marker is not part of the supported YAML subset"));
        }
        if self.started {
            return Err(self.err("a second document is not part of the supported YAML subset"));
        }
        self.started = true;
        self.bump(3);
        self.skip_inline();
        // `--- a: 1` is a document marker with a mapping crammed after it,
        // which YAML does not allow and which skipping the line would eat.
        if !self.at_line_end() {
            return Err(self.err("a document marker must be alone on its line"));
        }
        self.skip_line();
        Ok(())
    }

    /// Everything after a value on its line: spaces, an optional comment, and
    /// the newline. Anything else means the value did not end where the
    /// parser thought it did.
    fn end_of_line(&mut self) -> Result<()> {
        self.skip_inline();
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

    /// One block node at a known indentation. `rest` is at the start of its
    /// first line and `indent` is that line's indentation, as reported by
    /// `next_content`.
    fn block(&mut self, indent: usize) -> Result<Value> {
        let head = &self.rest[indent..];
        // A flow collection is allowed to sit on the line *under* its key —
        // `resolution:\n  {integrity: …}` is the same node as the inline
        // `resolution: {integrity: …}` pnpm actually writes, and any YAML
        // reformatter may produce it. Without this arm the line fell to
        // `mapping`, whose key scanner stopped at the first `: ` and handed
        // back the key `{integrity`. The package then had no `integrity`, so
        // its origin dropped to `Elsewhere` and `rules::slopsquat` skipped it
        // outright: a legal reformat of a lockfile turned findings off.
        if matches!(head.as_bytes().first(), Some(b'{' | b'[')) {
            self.bump(indent);
            let v = self.inline_value()?;
            self.end_of_line()?;
            return Ok(v);
        }
        if is_dash(head) {
            self.sequence(indent)
        } else {
            self.mapping(indent)
        }
    }

    fn mapping(&mut self, indent: usize) -> Result<Value> {
        self.enter()?;
        let mut map: BTreeMap<String, Value> = BTreeMap::new();

        while let Some(n) = self.next_content()? {
            // Less indented: this line belongs to an enclosing block, and it
            // is that block's job to decide whether the amount is legal. More
            // indented: nothing opened a deeper level, so there is no block
            // for the line to be in. That second case is the one an
            // inconsistently indented file hits, and guessing at it — by
            // treating the deeper line as a sibling, which is what a
            // forgiving parser does — silently reparents a key.
            if n < indent {
                break;
            }
            if n > indent {
                let at = &self.rest[n..];
                return Err(self.err_at(at, "unexpected indentation"));
            }
            self.bump(n);

            if is_dash(self.rest) {
                return Err(
                    self.err("a block sequence must be indented under its key, not level with it")
                );
            }

            let key_at = self.rest;
            let key = self.key()?;
            self.skip_inline();

            let value = if self.at_line_end() {
                self.end_of_line()?;
                // A bare `key:` takes whatever block is indented under it. If
                // the next content line is not deeper, the key has no value —
                // which is how `importers:` and `packages:` open their blocks
                // and how `key:` at the end of a file resolves to null.
                match self.next_content()? {
                    Some(deeper) if deeper > indent => self.block(deeper)?,
                    _ => Value::Null,
                }
            } else {
                let v = self.inline_value()?;
                self.end_of_line()?;
                v
            };

            if map.insert(key.clone(), value).is_some() {
                return Err(self.err_at(key_at, format!("duplicate key `{key}`")));
            }
        }

        self.depth -= 1;
        Ok(Value::Mapping(map))
    }

    fn sequence(&mut self, indent: usize) -> Result<Value> {
        self.enter()?;
        let mut items = Vec::new();

        while let Some(n) = self.next_content()? {
            if n < indent {
                break;
            }
            if n > indent {
                let at = &self.rest[n..];
                return Err(self.err_at(at, "unexpected indentation"));
            }
            self.bump(n);
            if !is_dash(self.rest) {
                return Err(self.err("expected '- ' to continue the sequence"));
            }
            self.bump(1);
            self.skip_inline();
            if self.at_line_end() {
                return Err(self.err("expected a value after '-'"));
            }
            // A `- ` item is a scalar or a flow collection and nothing else.
            // `inline_value` refuses an unquoted `: `, which is what turns
            // `- name: 1` into a refusal rather than the string "name: 1".
            let v = self.inline_value()?;
            self.end_of_line()?;
            items.push(v);
        }

        self.depth -= 1;
        Ok(Value::Sequence(items))
    }

    fn at_line_end(&self) -> bool {
        matches!(self.peek(), None | Some(b'\n' | b'\r' | b'#'))
    }

    /// One mapping key, up to and including the `:` that ends it.
    fn key(&mut self) -> Result<String> {
        let key = match self.peek() {
            Some(b'\'') => self.single_quoted()?,
            Some(b'"') => self.double_quoted()?,
            _ => self.plain_key()?,
        };
        self.skip_inline();
        if self.peek() != Some(b':') {
            return Err(self.err("expected ':' after a mapping key"));
        }
        self.bump(1);
        Ok(key)
    }

    /// An unquoted key runs to the first `:` that is followed by whitespace or
    /// end of line.
    ///
    /// That "followed by" clause is the whole trick, and splitting on the
    /// first `:` instead is the standard way to get this wrong. pnpm keys are
    /// package identifiers: `zwitch@2.0.4`, `acorn-jsx@5.3.2(acorn@8.14.1)`,
    /// `.` for the root importer. None of those contain a `: `, but a key that
    /// did — `a:b` is a perfectly legal YAML key — would be cut in half by a
    /// naive split and the rest of the line read as its value.
    fn plain_key(&mut self) -> Result<String> {
        self.reject_indicator()?;
        self.reject_flow_indicator()?;
        let line = self.line();
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b':' && breaks_after(bytes, i + 1) {
                break;
            }
            // ` #` opens a comment, so the key ended before it and there is
            // no ':' on this line.
            if bytes[i] == b'#' && i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
                return Err(self.err("expected ':' after a mapping key"));
            }
            i += 1;
        }
        if i == bytes.len() {
            return Err(self.err("expected ':' after a mapping key"));
        }
        let key = line[..i].trim_end_matches([' ', '\t']);
        if key.is_empty() {
            return Err(self.err("empty mapping key"));
        }
        self.bump(i);
        Ok(key.to_string())
    }

    /// A value that starts on the same line as its key or its `-`.
    fn inline_value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'\'') => Ok(Value::String(self.single_quoted()?)),
            Some(b'"') => Ok(Value::String(self.double_quoted()?)),
            Some(b'{') => self.flow_mapping(),
            Some(b'[') => self.flow_sequence(),
            _ if is_dash(self.rest) => {
                Err(self.err("a sequence item may not start on its parent's line"))
            }
            _ => self.plain_scalar(),
        }
    }

    /// The YAML indicators a plain scalar may not open with.
    ///
    /// This is the guard that keeps the subset honest. Left to itself the
    /// plain-scalar scanner reads `&anchor 1` as the six-character string
    /// "&anchor 1" and `*anchor` as "*anchor" — a file using anchors would
    /// parse, produce a tree, and be wrong in a way nothing downstream could
    /// notice. Anchors and aliases are the construct most likely to turn up in
    /// hand-written YAML, so they are refused by name rather than left to fail
    /// somewhere else, or worse, not fail at all.
    fn reject_indicator(&self) -> Result<()> {
        let what = match self.peek() {
            Some(b'&') => "anchors are not part of the supported YAML subset",
            Some(b'*') => "aliases are not part of the supported YAML subset",
            Some(b'!') => "tags are not part of the supported YAML subset",
            Some(b'|' | b'>') => "block scalars are not part of the supported YAML subset",
            Some(b'%') => "directives are not part of the supported YAML subset",
            Some(b'@' | b'`') => "this character is reserved in YAML; quote the scalar",
            _ => return Ok(()),
        };
        Err(self.err(what))
    }

    /// The flow indicators, which no plain scalar may open with either — YAML
    /// puts `,[]{}` in `c-indicator`, so this is the spec's rule and not a
    /// local convenience.
    ///
    /// It is a second function rather than five more arms on
    /// `reject_indicator` because inside `{}` and `[]` these five bytes are
    /// structure, and the flow parsers already say something better about
    /// them: `[,]` comes back as "expected a value" and `{,}` as "expected
    /// ':' after a flow mapping key", both pointing at the character. Only the
    /// block-context scanners want this, so only they call it.
    fn reject_flow_indicator(&self) -> Result<()> {
        match self.peek() {
            Some(b'{' | b'}' | b'[' | b']' | b',') => {
                Err(self.err("a flow indicator may not open a plain scalar; quote it"))
            }
            _ => Ok(()),
        }
    }

    /// A plain scalar in block context: everything to end of line, minus a
    /// trailing comment and trailing whitespace.
    ///
    /// The one thing it will not swallow is an unquoted `: `. YAML forbids it
    /// here too ("mapping values are not allowed in this context"), and
    /// refusing it is what keeps `- name: 1` and `a: b: c` from turning into
    /// strings that look like they parsed.
    fn plain_scalar(&mut self) -> Result<Value> {
        self.reject_indicator()?;
        self.reject_flow_indicator()?;
        let line = self.line();
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b':' && breaks_after(bytes, i + 1) {
                let at = &self.rest[i..];
                return Err(self.err_at(at, "':' in a plain scalar; quote it, or start a block"));
            }
            if bytes[i] == b'#' && i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
                break;
            }
            i += 1;
        }
        let text = line[..i].trim_end_matches([' ', '\t']);
        self.bump(i);
        Ok(scalar(text))
    }

    fn single_quoted(&mut self) -> Result<String> {
        let open = self.rest;
        self.bump(1);
        let mut out = String::new();
        loop {
            match self.peek() {
                None | Some(b'\n' | b'\r') => {
                    return Err(self.err_at(open, "unterminated single-quoted scalar"));
                }
                // The only escape a single-quoted scalar has.
                Some(b'\'') if self.rest.as_bytes().get(1) == Some(&b'\'') => {
                    out.push('\'');
                    self.bump(2);
                }
                Some(b'\'') => {
                    self.bump(1);
                    return Ok(out);
                }
                Some(_) => {
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

    /// YAML's double-quoted escapes are a superset of JSON's — `\e`, `\x41`,
    /// `\N`, `\_`, a backslash at end of line to fold. Only the JSON set plus
    /// `\0` is accepted; the rest are refused by name so a file that uses one
    /// gets told which.
    fn double_quoted(&mut self) -> Result<String> {
        let open = self.rest;
        self.bump(1);
        let mut out = String::new();
        loop {
            match self.peek() {
                None | Some(b'\n' | b'\r') => {
                    return Err(self.err_at(open, "unterminated double-quoted scalar"));
                }
                Some(b'"') => {
                    self.bump(1);
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.bump(1);
                    self.escape(&mut out)?;
                }
                Some(_) => {
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
            b'0' => Some('\0'),
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
            return Err(self.err(format!("unsupported escape `\\{}`", b as char)));
        }
        self.bump(1);
        let digits = self
            .rest
            .get(..4)
            .ok_or_else(|| self.err("truncated \\u escape"))?;
        let mut n: u32 = 0;
        for d in digits.bytes() {
            let v = match d {
                b'0'..=b'9' => d - b'0',
                b'a'..=b'f' => d - b'a' + 10,
                b'A'..=b'F' => d - b'A' + 10,
                _ => return Err(self.err("\\u escape needs four hex digits")),
            };
            n = n * 16 + v as u32;
        }
        self.bump(4);
        // Unlike JSON, YAML has no surrogate-pair convention — `\uD800` is
        // simply not a character. Rejecting it beats substituting U+FFFD into
        // a package name.
        let c =
            char::from_u32(n).ok_or_else(|| self.err("escape is not a Unicode scalar value"))?;
        out.push(c);
        Ok(())
    }

    fn flow_mapping(&mut self) -> Result<Value> {
        self.enter()?;
        let open = self.rest;
        self.bump(1);
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        loop {
            // Looking for the close before every entry rather than only after
            // one is what makes `{}` and a trailing comma fall out for free —
            // the same shape `toml::array` uses, and YAML allows both.
            self.skip_inline();
            match self.peek() {
                Some(b'}') => {
                    self.bump(1);
                    break;
                }
                None => return Err(self.err_at(open, "unclosed flow mapping")),
                Some(b'\n' | b'\r') => {
                    return Err(self.err_at(open, "a flow mapping may not span lines"));
                }
                _ => {}
            }
            let key_at = self.rest;
            let key = self.flow_key(open)?;
            self.skip_inline();
            let value = self.flow_value()?;
            if map.insert(key.clone(), value).is_some() {
                return Err(self.err_at(key_at, format!("duplicate key `{key}`")));
            }
            self.skip_inline();
            match self.peek() {
                Some(b',') => self.bump(1),
                Some(b'}') => {
                    self.bump(1);
                    break;
                }
                None => return Err(self.err_at(open, "unclosed flow mapping")),
                Some(b'\n' | b'\r') => {
                    return Err(self.err_at(open, "a flow mapping may not span lines"));
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Mapping(map))
    }

    fn flow_key(&mut self, open: &str) -> Result<String> {
        let key = match self.peek() {
            Some(b'\'') => self.single_quoted()?,
            Some(b'"') => self.double_quoted()?,
            _ => {
                // The one unquoted-text scanner that never asked. `{&a b: 1}`,
                // `{*a: 1}` and `{!!str b: 1}` all came back as key text, so a
                // flow mapping was the way to smuggle an anchor past a parser
                // that refuses anchors everywhere else.
                self.reject_indicator()?;
                // Scanning `self.rest` and not `self.line()`. This loop runs
                // once per entry and `line()` walks to the newline every time,
                // so the cost of one key grew with the length of the whole
                // line: 64,000 entries on one line took 99.7 s in release, and
                // 92 ms once `\n` and `\r` became two more arms in the match
                // below. `line()` is still right for the block-context
                // scanners, which really do read to end of line.
                let bytes = self.rest.as_bytes();
                let mut i = 0;
                let key_end = loop {
                    match bytes.get(i) {
                        // Out of input, or off the end of the line, with no
                        // `:` — either way the brace never closed.
                        None => return Err(self.err_at(open, "unclosed flow mapping")),
                        Some(b'\n' | b'\r') => {
                            return Err(self.err_at(open, "a flow mapping may not span lines"));
                        }
                        Some(b':') if breaks_after(bytes, i + 1) => break i,
                        Some(b',' | b'{' | b'}' | b'[' | b']') => {
                            let at = &self.rest[i..];
                            return Err(self.err_at(at, "expected ':' after a flow mapping key"));
                        }
                        // ` #` opens a comment here too — this is the fourth of
                        // four scalar scanners and was the one that read
                        // `{b #x: 1}` as the key "b #x". The key ends at the
                        // space; the `:` after it is inside a comment, so the
                        // `expected ':'` check below is what reports it.
                        Some(b'#') if i > 0 && matches!(bytes[i - 1], b' ' | b'\t') => break i,
                        Some(_) => i += 1,
                    }
                };
                let k = self.rest[..key_end]
                    .trim_end_matches([' ', '\t'])
                    .to_string();
                // `plain_key` refuses an empty key in block context and this
                // scanner accepted one, so `a: {: 1}` built the key "" while
                // `a:\n  : 1` was an error. Not an invented key — the text
                // really is empty — but two key scanners disagreeing about the
                // same question is the kind of gap somebody finds by probing.
                if k.is_empty() {
                    return Err(self.err("empty mapping key"));
                }
                self.bump(key_end);
                k
            }
        };
        self.skip_inline();
        if self.peek() != Some(b':') {
            return Err(self.err("expected ':' after a flow mapping key"));
        }
        self.bump(1);
        Ok(key)
    }

    fn flow_sequence(&mut self) -> Result<Value> {
        self.enter()?;
        let open = self.rest;
        self.bump(1);
        let mut items = Vec::new();
        loop {
            // As in `flow_mapping`: `[]` and `[x, ]` both come out of checking
            // for the close first. It also puts the line-spanning refusal on
            // the path a broken-after-a-comma sequence takes, which used to
            // reach `plain_flow_scalar` and come back as "expected a value".
            self.skip_inline();
            match self.peek() {
                Some(b']') => {
                    self.bump(1);
                    break;
                }
                None => return Err(self.err_at(open, "unclosed flow sequence")),
                Some(b'\n' | b'\r') => {
                    return Err(self.err_at(open, "a flow sequence may not span lines"));
                }
                _ => {}
            }
            items.push(self.flow_value()?);
            self.skip_inline();
            match self.peek() {
                Some(b',') => self.bump(1),
                Some(b']') => {
                    self.bump(1);
                    break;
                }
                None => return Err(self.err_at(open, "unclosed flow sequence")),
                Some(b'\n' | b'\r') => {
                    return Err(self.err_at(open, "a flow sequence may not span lines"));
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Sequence(items))
    }

    fn flow_value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'\'') => Ok(Value::String(self.single_quoted()?)),
            Some(b'"') => Ok(Value::String(self.double_quoted()?)),
            Some(b'{') => self.flow_mapping(),
            Some(b'[') => self.flow_sequence(),
            _ => self.plain_flow_scalar(),
        }
    }

    /// Plain scalar inside `{}` or `[]`. Ends at a comma or a closing
    /// bracket rather than at end of line, which is why it cannot share code
    /// with the block version.
    ///
    /// It has to hold `sha512-30iZ…YYw==` and `^18.17.1 || ^20.3.0 || >=22.0.0`
    /// intact — 1,570 of the fixture's lines depend on it — while still
    /// refusing an unquoted `: `.
    fn plain_flow_scalar(&mut self) -> Result<Value> {
        self.reject_indicator()?;
        // As in `flow_key`, over `self.rest` rather than `self.line()`. This
        // scanner breaks at the first `,` or bracket, so every `line()` call
        // threw its whole walk away — once per item. One 1 MB `os: [...]` line
        // took 471.8 s in release that way, and 139 ms this way. `\n` and `\r`
        // join the break set; the caller was going to refuse them anyway.
        let bytes = self.rest.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b',' | b'{' | b'}' | b'[' | b']' | b'\n' | b'\r' => break,
                b':' if breaks_after(bytes, i + 1) => {
                    let at = &self.rest[i..];
                    return Err(self.err_at(at, "':' in a flow scalar; quote it"));
                }
                b'#' if i > 0 && matches!(bytes[i - 1], b' ' | b'\t') => break,
                _ => i += 1,
            }
        }
        let text = self.rest[..i].trim_end_matches([' ', '\t']);
        if text.is_empty() {
            return Err(self.err("expected a value"));
        }
        self.bump(i);
        Ok(scalar(text))
    }
}

/// The two tokens this parser types. See the module comment for why the list
/// stops here.
fn scalar(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(text.to_string()),
    }
}

/// `---` or `...` standing alone: YAML's document markers. The trailing break
/// is what keeps `---foo: 1` and `...: 1` ordinary keys.
fn document_marker(s: &str) -> Option<&'static str> {
    let marker = ["---", "..."].into_iter().find(|m| s.starts_with(m))?;
    breaks_after(s.as_bytes(), 3).then_some(marker)
}

/// A `-` that opens a block sequence entry, as opposed to one that starts a
/// scalar like `-1` or `-rc.1`.
fn is_dash(s: &str) -> bool {
    let b = s.as_bytes();
    b.first() == Some(&b'-') && breaks_after(b, 1)
}

/// Whether index `i` is end of line or whitespace — the test that turns a `:`
/// into a mapping indicator and leaves every other `:` inside the scalar.
fn breaks_after(bytes: &[u8], i: usize) -> bool {
    match bytes.get(i) {
        None => true,
        Some(b' ' | b'\t' | b'\n' | b'\r') => true,
        Some(_) => false,
    }
}
