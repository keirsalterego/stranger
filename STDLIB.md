# STDLIB.md

Crates `stranger` would have installed, and what it used instead.

Every download figure is from the crates.io API, measured **2026-08-28**. crates.io
reports all-time downloads and a 90-day `recent_downloads`; it does not report a
weekly number, so those two are what is quoted rather than a figure invented to
look like npm's.

The "what I gave up" column is the one worth reading. Some of these substitutions
are genuinely better than the crate. Most are worse in some specific way, and that
way is named.

> **This file grows as modules land**, not on the last night. An entry written the
> hour the module was written says what was actually traded; ten reconstructed on
> Sunday read like ten reconstructed on Sunday.

---

## The nomination: `serde_json`

**1,227,048,507 all-time · 288,758,389 in 90 days**

Replaced by [`src/json.rs`](src/json.rs), a complete RFC 8259 reader: escapes,
`\uXXXX` with surrogate-pair recombination, the number grammar, nesting limits,
and byte-offset-derived line/column on every error.

The position tracking is the part worth defending. The parser keeps the original
input alongside the unconsumed remainder and asks the standard library where one
sits inside the other — `str::substr_range`, stable in 1.98. There is no cursor
struct threaded through thirty functions and no line counter to keep in sync,
because the cursor *is* the remainder.

The number grammar is checked by hand before `f64::from_str` sees the slice.
Rust's float parser accepts `inf`, `NaN`, `1.`, `.5` and `+1`; JSON accepts none
of them. `tests/json.rs` tests exactly that list.

**What I gave up:** `serde`'s derive machinery. There is no `#[derive(Deserialize)]`
here — every lockfile reader walks a `Value` tree by hand and decides what to do
with a missing field at the point it is missing. For six readers that is fine and
arguably clearer, since the "what if this key is absent" question gets answered
where the answer matters instead of in an attribute. For a hundred types it would
be miserable. I also gave up streaming: the whole file is parsed into memory, which
is correct for a 718 KB lockfile and wrong for a 700 MB one.

Numbers are `f64`, so a JSON integer beyond 2^53 loses precision. Nothing in a
lockfile is such a number — versions and hashes are strings — but it is a real
limitation and not a theoretical one for other inputs.

---

## Parsing and CLI

### `clap` — 1,083,204,108 all-time · 222,262,381 in 90 days
[`src/cli.rs`](src/cli.rs). A hand-written parser for one subcommand, five flags
and three exit codes.

**What I gave up:** shell completions, `--help` generated from the same source as
the parser (so mine can drift, and only a test stops it), colored help, and
`clap`'s genuinely good error messages for typo'd flags. What I got back is error
text that says what was expected *here* — `--format takes 'human' or 'json', not
'jsonn'` — rather than reprinting a grammar.

### `anyhow` — 909,556,524 all-time · 200,860,233 in 90 days
### `thiserror` — 1,377,720,340 all-time · 338,962,699 in 90 days
[`src/error.rs`](src/error.rs). One enum, three variants, hand-written `Display`
and `std::error::Error` with a real `source()`.

**What I gave up:** `thiserror`'s derive, which for three variants is about
fifteen lines of boilerplate — a fair trade. `anyhow`'s backtrace capture and its
`.context()` chaining, which I miss more; adding a file path to an IO error is
manual here. What I got is that `Error::Syntax` carries `line` and `col` as
structured fields rather than a formatted string, so the tests can assert on
position without parsing the message back out.

---

## Text

### `strsim` — 1,024,185,642 all-time · 209,773,661 in 90 days
[`src/distance.rs`](src/distance.rs). Damerau-Levenshtein with a length prefilter
and a row-minimum early exit.

**What I gave up:** nothing, and I think this one is actually better. `strsim`'s
`damerau_levenshtein` is the *optimal string alignment* variant, which is what
nearly every implementation ships under that name. OSA is not a metric — it fails
the triangle inequality, and it scores `CA` against `ABC` as 3 when the true
distance is 2. `src/distance.rs` implements the unrestricted Lowrance-Wagner
version, which is a metric, and `tests/distance.rs` proves it over 20,000 random
triples. That cost about fifteen lines.

What I did give up is `strsim`'s other seven algorithms — Jaro, Jaro-Winkler,
Sørensen-Dice — which I would want if the corpus matching ever needed a
similarity ratio rather than an edit count.

### `semver` — 945,451,453 all-time · 198,517,936 in 90 days
[`src/semver.rs`](src/semver.rs). Parsing, precedence, and the caret and tilde
operators.

**What I gave up:** multi-comparator ranges. `semver` handles `>=1.2, <1.5` and
`1.2.x || 2.x`; mine handles one operator at a time, because one operator is what
a lockfile pin and a `requirements.txt` line contain. Compound npm ranges from
`package.json` would need more.

The part I did not give up is prerelease precedence, which is where
implementations usually go wrong. `1.0.0-beta.11 > 1.0.0-beta.2` requires numeric
segments to compare as numbers rather than strings; `1.0.0-1 < 1.0.0-alpha`
requires numeric to sort *below* alphanumeric; and `1.0.0-rc.1 < 1.0.0` requires
the empty prerelease list to be the largest rather than the smallest, which
inverts the usual intuition. `tests/semver.rs` runs the exact ordering table from
semver.org section 11, both pairwise and as a sort.

### `itoa` — 1,265,455,201 all-time · 292,076,583 in 90 days
[`src/report.rs`](src/report.rs), `thousands()`. `core::fmt::NumBuffer` plus
`format_into`, stable in 1.98, writes digits into a stack buffer with no
allocation and no `Display` machinery.

**What I gave up:** nothing measurable. The 1.98 toolchain killed this one for
free; the standard library benchmarks `format_into` on par with `itoa`. Listing
it because it is an honest substitution, not because it was hard.

---

## State and concurrency

### `once_cell` — 1,187,857,958 all-time · 262,379,838 in 90 days
[`src/corpus.rs`](src/corpus.rs). `std::sync::LazyLock`, stable since 1.80. The
three corpora load through one each.

**What I gave up:** nothing. `once_cell::sync::Lazy` and `LazyLock` are the same
thing, and people still reach for the crate out of habit. Another one the
toolchain killed for free.

### `rand` — 1,605,926,795 all-time · 401,565,502 in 90 days
[`tests/distance.rs`](tests/distance.rs) and [`tests/ablation.rs`](tests/ablation.rs).
A five-line xorshift64\*, seeded from `SystemTime` nanoseconds and printed so a
failing case replays.

**What I gave up:** a great deal, and it does not matter here. xorshift64\* is not
cryptographically secure, has a much shorter period than ChaCha, and fails some
statistical tests `rand`'s generators pass. It is used to generate random short
strings for property tests and to thin a corpus deterministically. If anything in
`stranger` ever needed randomness for a security decision, this would be the wrong
tool — but nothing does, because the tool has no secrets and makes no nonces.

Worth stating plainly since the hackathon rules forbid rolling your own crypto:
this is not crypto and is not used as crypto. `stranger` computes no hashes,
verifies no signatures, and has no key material.

---

## Not yet claimed

Entries for `toml`, `serde_yaml`, `semver`, `owo-colors`, `comfy-table`,
`is-terminal`, `walkdir`, `glob`, `indicatif`, `rayon` and `crossbeam-channel`
land as their modules do. An unwritten module gets no entry.

---

## Data that is not code, disclosed anyway

`corpus/` holds 160,066 package names across three ecosystems, fetched once with
`curl` at development time on 2026-08-28. `fixtures/` holds fourteen real
lockfiles from public projects plus two poisoned by hand.

Neither is code. Nothing in either directory is compiled as source — the corpora
are embedded with `include_str!` and read as text, and the fixtures are test
input. Full provenance, including the exact endpoints and the two dead ends that
cost an hour, is in [`corpus/PROVENANCE.md`](corpus/PROVENANCE.md) and
[`fixtures/README.md`](fixtures/README.md).

Disclosed here as well because the rule is that anything not written this weekend
gets disclosed in STDLIB.md, and a name list is exactly the sort of thing it would
be convenient to forget.

The tool degrades honestly without the corpus: with an empty list every name is
"not in corpus", clause two finds no neighbour, and the rule reports nothing
rather than everything. `Ecosystem::Go` already runs in that state deliberately.
