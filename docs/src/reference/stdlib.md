# The STDLIB log

Eighteen crates, and what the standard library did instead.

The canonical file is
[STDLIB.md](https://github.com/keirsalterego/stranger/blob/main/STDLIB.md), and it
carries the part that matters: an honest **what I gave up** paragraph on every
entry. This page is the index — what was replaced, by what, and where the code is.
It deliberately does not repeat the rationale, because two copies of an argument
drift and one of them starts lying.

Download counts are what the crates.io API returned on **2026-08-28**, quoted in
the two forms it actually reports. It has no weekly figure, so none is invented.

## The nomination

**`serde_json` — 1,227,048,507 all-time · 288,758,389 in 90 days**

[`src/json.rs`](https://github.com/keirsalterego/stranger/blob/main/src/json.rs) is
a complete RFC 8259 reader: escapes, `\uXXXX` with surrogate-pair recombination,
the number grammar checked by hand before `f64::from_str` sees the slice, a nesting
limit, and line/column on every error derived from a byte offset.

The position tracking is the part worth defending. The parser keeps the original
input alongside the unconsumed remainder and asks the standard library where one
sits inside the other — `str::substr_range`, stable in 1.98. No cursor struct
threaded through thirty functions, no line counter to keep in sync, because the
cursor *is* the remainder.

## The other seventeen

| crate | all-time | replaced by | what did the work |
|---|---|---|---|
| `rand` | 1,605,926,795 | `src/distance.rs`, `tests/` | five lines of xorshift64\*, seeded from `SystemTime` |
| `thiserror` | 1,377,720,340 | `src/error.rs` | one enum, hand-written `Display` and `Error::source` |
| `itoa` | 1,265,455,201 | `src/report.rs` | `core::fmt::NumBuffer` + `format_into` (1.98) |
| `once_cell` | 1,187,857,958 | `src/corpus.rs` | `std::sync::LazyLock` (1.80) |
| `clap` | 1,083,204,108 | `src/cli.rs` | a match over `std::env::args` |
| `strsim` | 1,024,185,642 | `src/distance.rs` | unrestricted Damerau-Levenshtein, written out |
| `semver` | 945,451,453 | `src/semver.rs` | the precedence rules from the spec's section 11 |
| `anyhow` | 909,556,524 | `src/error.rs` | the same enum, with `?` |
| `toml` | 855,052,855 | `src/toml.rs` | a documented subset that refuses what it cannot read |
| `walkdir` | 588,096,443 | `src/walk.rs` | `std::fs::read_dir` and an explicit stack |
| `glob` | 575,867,559 | `src/walk.rs` | `str::ends_with` against seven known names |
| `crossbeam-channel` | 565,386,665 | `src/main.rs` | `std::sync::mpsc` |
| `rayon` | 516,056,344 | `src/main.rs` | `std::thread::scope` |
| `serde_yaml` | 383,697,832 | `src/yaml.rs` | an indentation-driven subset, two booleans only |
| `is-terminal` | 324,499,410 | `src/term.rs` | `std::io::IsTerminal` (1.70) |
| `owo-colors` | 156,700,441 | `src/term.rs` | sixteen-colour SGR, written out |
| `comfy-table` | 94,630,283 | `src/term.rs` | column widths measured from content |

**13,459,345,853 all-time downloads** across those seventeen, and
**14,686,394,360** with the nomination. The bonus asks for ten entries.

## Three of these were free, and saying so is the point

`itoa` → `NumBuffer::format_into` and `once_cell` → `LazyLock` are substitutions
the toolchain made for me. `LazyLock` landed in 1.80 and `format_into` in 1.98,
eight days before the window opened. Neither cost an hour, and claiming them as
craft would be claiming a stable release as personal work.

`is-terminal` is the third and it is the interesting one, because avoiding it
avoided `unsafe`. The usual replacement is an FFI call to `libc::isatty`, which
needs an `unsafe` block and would have broken `#![forbid(unsafe_code)]` at both
crate roots. `std::io::IsTerminal` has done it safely since 1.70.

## What is not claimed

`indicatif`. Nothing here draws a progress bar: the largest fixture scans in under
half a second, and a bar that finishes before it renders is a dependency bought for
one frame. Writing a spinner so that a spinner crate could be crossed off is
padding rather than substitution.

The rule the log enforces on itself is that an unwritten module gets no entry. That
is why `src/semver.rs` **is** in the table — it exists, it has 13 tests — while the
page on [Limits](../limits.md) says plainly that nothing calls it.

## Data that is not code, disclosed anyway

`corpus/` holds 160,066 package names across three registries, fetched once with
`curl` at development time on 2026-08-28. `fixtures/` holds fourteen real lockfiles
from public projects plus two poisoned by hand.

Neither is code — the corpora are embedded with `include_str!` and read as text,
and the fixtures are test input. The hackathon's rule is that anything not written
during the window is disclosed in `STDLIB.md` or it scores against you, and a name
list is exactly the sort of thing it would be convenient to forget. Full provenance
is in `corpus/PROVENANCE.md` and `fixtures/README.md`.

```console
$ wc -l corpus/*.txt
    5000 corpus/crates-io.txt
  140066 corpus/npm.txt
   15000 corpus/pypi.txt
  160066 total
```
