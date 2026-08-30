//! Terminal colour and column layout, hand-written.
//!
//! Three crates normally cover this — `owo-colors`, `comfy-table`,
//! `is-terminal`. None of them earns its place here: the whole of
//! `is-terminal` has been in std since 1.70, colour is four SGR strings, and a
//! two-column table is a `max` and some spaces.

use std::borrow::Cow;
use std::io::IsTerminal;

const RESET: &str = "\x1b[0m";

/// Sixteen-colour SGR only. These render the way the reader's theme intends;
/// a truecolor escape renders as whatever the author's monitor looked like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Red,
    Orange,
    Yellow,
    Dim,
}

impl Style {
    fn code(self) -> &'static str {
        match self {
            Style::Red => "\x1b[31m",
            Style::Orange => "\x1b[93m",
            Style::Yellow => "\x1b[33m",
            Style::Dim => "\x1b[2m",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Term {
    color: bool,
}

impl Term {
    pub fn new(color: bool) -> Self {
        Term { color }
    }

    /// Read the environment once, at the top of the program. Everything
    /// downstream takes the answer as an argument, so a render is reproducible
    /// from what was passed to it.
    pub fn detect(cli_no_color: bool) -> Self {
        let no_color = std::env::var("NO_COLOR").ok();
        let force = std::env::var("CLICOLOR_FORCE").ok();
        // `std::io::IsTerminal`, stable since 1.70 — this line is the entire
        // `is-terminal` crate. The alternative is an `isatty` FFI declaration,
        // and calling it needs an `unsafe` block, which the crate root forbids.
        let tty = std::io::stdout().is_terminal();
        Term::new(decide(
            cli_no_color,
            no_color.as_deref(),
            force.as_deref(),
            tty,
        ))
    }

    pub fn paint<'a>(self, style: Style, text: &'a str) -> Cow<'a, str> {
        if self.color {
            Cow::Owned(format!("{}{text}{RESET}", style.code()))
        } else {
            Cow::Borrowed(text)
        }
    }
}

/// Whether to emit escape codes, as a pure function of its four inputs.
///
/// The order below is a decision, not a standard: no-color.org and the
/// CLICOLOR convention were written independently and neither ranks itself
/// against the other. Highest priority first —
///
///   1. `--no-color`      the person running it said so, out loud, this run.
///   2. `NO_COLOR`        off regardless of TTY.
///   3. `CLICOLOR_FORCE`  on regardless of TTY.
///   4. stdout is a TTY.
///
/// Off beats on at every tie, because a stray `CLICOLOR_FORCE` in a CI image
/// should not be able to spray escape codes into a log that asked for none.
/// Both variables count only when present *and* non-empty, so `NO_COLOR=`
/// means nothing was said rather than "off" — that is what no-color.org asks
/// for, and it is how a shell clears a variable it cannot `unset`.
///
/// Taking the environment as arguments rather than reading it here is what
/// makes the table testable: `std::env::set_var` is `unsafe` in edition 2024,
/// and this crate forbids unsafe.
pub fn decide(
    cli_no_color: bool,
    no_color: Option<&str>,
    clicolor_force: Option<&str>,
    tty: bool,
) -> bool {
    if cli_no_color || no_color.is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if clicolor_force.is_some_and(|v| !v.is_empty()) {
        return true;
    }
    tty
}

/// Untrusted text, safe to hand to a terminal.
///
/// A lockfile is a file written by strangers — that is the whole premise — and
/// the human report prints its package names and versions to a TTY. A version
/// string of `1.0.0\x1b[2K\x1b[1A\x1b[2K` erases the two lines above it: the
/// `HALLUCINATION RISK` heading and the package name scroll out of existence
/// while the exit code still says 1. `\x1b[2J` clears the screen. A tool whose
/// findings can be deleted by the file it is auditing is not an auditing tool,
/// and this is the one bug in the repo that a *malicious* input could exploit
/// rather than merely trip over.
///
/// Replaced with U+FFFD rather than dropped, so the cell still takes a column
/// and the reader can see that something was there. Nothing real is lost: no
/// registry permits a control character in a name — npm allows URL-safe
/// characters, PyPI normalises to `[a-z0-9.-]`, crates.io to `[A-Za-z0-9_-]` —
/// and no version scheme has one either. DEL and the C1 range go too, because
/// `\u{9b}` is a single-byte CSI on a terminal decoding Latin-1.
///
/// Borrowed when there is nothing to do, which is every name in every fixture.
pub fn sanitize(s: &str) -> Cow<'_, str> {
    if !s.chars().any(is_control) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| if is_control(c) { '\u{fffd}' } else { c })
            .collect(),
    )
}

fn is_control(c: char) -> bool {
    matches!(c, '\0'..='\x1f' | '\x7f'..='\u{9f}')
}

/// Display width of a cell: one column per Unicode scalar.
///
/// `chars().count()` — wrong for three things. East Asian wide and
/// fullwidth forms take two columns, combining marks take zero, and an emoji
/// ZWJ sequence takes two however many scalars it is built from. Getting those
/// right means shipping a table derived from `EastAsianWidth.txt` — tens of
/// kilobytes of generated data plus a Unicode version to keep current — to
/// align a column of package names. Registry names are ASCII in practice: npm
/// permits only URL-safe characters, PyPI normalises to `[a-z0-9.-]`,
/// crates.io to `[A-Za-z0-9_-]`. So this is exact for every name a lockfile
/// can hand us, and counting bytes would not be — a name with an accent in it
/// still lines up. Upgrade path: generate the table at build time if a
/// registry that permits CJK identifiers ever turns up.
///
/// Assumes its argument came through `sanitize`. An escape sequence counted as
/// eight columns is how a hostile version string knocked every row after it out
/// of alignment, so the two belong together.
pub fn width(s: &str) -> usize {
    s.chars().count()
}

/// Widest cell in a column, never narrower than `min`.
pub fn column<'a>(cells: impl IntoIterator<Item = &'a str>, min: usize) -> usize {
    cells.into_iter().fold(min, |w, c| w.max(width(c)))
}

/// `s` followed by enough spaces to reach `to`. A cell wider than its column
/// pushes the row out instead of being truncated — the name is the one thing
/// in a finding the reader has to be able to copy.
pub fn pad(s: &str, to: usize) -> String {
    let mut out = String::with_capacity(s.len() + to);
    out.push_str(s);
    for _ in width(s)..to {
        out.push(' ');
    }
    out
}
