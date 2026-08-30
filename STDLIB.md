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

## Package Killer nomination: `serde_json`

**1,227,048,507 all-time · 288,758,389 in 90 days**

`rand` is bigger — 1,605,926,795 — and it is deliberately not the nomination.
The bonus asks for one crate nailed, and five lines of xorshift is not a case
for anything. The nomination goes to the substitution with the most work behind
it, not to the largest number in the table.

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
with a missing field at the point it is missing. For eight readers that is fine
and arguably clearer, since the "what if this key is absent" question gets answered
where the answer matters instead of in an attribute. For a hundred types it would
be miserable. I also gave up streaming: the whole file is parsed into memory, which
is correct for a 718 KB lockfile and wrong for a 700 MB one.

Numbers are `f64`, so a JSON integer beyond 2^53 loses precision. Nothing in a
lockfile is such a number — versions and hashes are strings — but it is a real
limitation and not a theoretical one for other inputs.

### The evidence, because "I wrote a parser" is not a case

Two things, both runnable.

**Clause by clause.** `tests/json_conformance.rs` walks the grammar and tests each
production, citing the section number in each test: the six structural characters,
all **eight** two-character escapes (the RFC lists eight, not six — `"` `\` `/`
`b` `f` `n` `r` `t`), `\uXXXX`, surrogate pairs and every way one can go wrong, the
number grammar including leading zeros and bare `+`, the literal names, whitespace,
nesting, and duplicate keys.

**Against a reference implementation.** `scripts/json-differential.sh` feeds
2,000,000 generated and mutated inputs to both this parser and CPython's `json`,
configured with `parse_constant` so it rejects `NaN` and `Infinity` — which RFC
8259 has neither of and Python accepts by default. **1,997,016 agreed. 2,984
disagreed, in four classes, and none of them was about a value:** every time both
accepted, both built the same thing down to the IEEE-754 bits.

| n | class | who is right |
|---|---|---|
| 1,093 | a leading BOM: we skip it, CPython raises | neither — §8.1 says a parser MAY ignore one, and skipping means a lockfile saved on Windows gets audited |
| 898 | lone high surrogate `"\ud83e"` | neither — §8.2 does not forbid unpaired surrogates, it calls them "not interoperable" and leaves it to the implementation |
| 825 | lone low surrogate | same |
| 168 | high surrogate followed by a non-surrogate escape | same |

Three of the four are the same decision: a Rust `String` cannot hold an unpaired
surrogate, so accepting one means WTF-8 or substituting U+FFFD, and silently
rewriting a character of a package name is precisely the bug class this tool
exists to find. Rejecting is the honest option, and the RFC permits it.

The campaign found **no defect** in `src/json.rs`. That is a result reported
rather than a claim made: the probes that went beyond the campaign — a
100,000-digit integer, a `\u` escape truncated at every offset, one whose fourth
byte falls mid-codepoint, the depth cap at 127, 128 and 129, and error columns
counted in characters behind multi-byte text — were all correct too.

`python3` is dev-time tooling for this and nothing else. It never enters
`Cargo.toml`, never ships, and the 29 clause tests run on a machine that does not
have it.

---

## Parsing and CLI

### `clap` — 1,083,204,108 all-time · 222,262,381 in 90 days
[`src/cli.rs`](src/cli.rs). A hand-written parser for three subcommands, the
options `USAGE` lists, and three exit codes. `--fail-on` means a different thing
on `scan` than on `diff` — the worst finding in the tree against the worst
finding the change introduced — which is a per-subcommand meaning a derive macro
makes you fight and a hand-written parser just writes down.

**What I gave up:** shell completions, `--help` generated from the same source as
the parser, colored help, and `clap`'s genuinely good error messages for typo'd
flags. The help text is a `const USAGE` string sitting in the same file as the
`match` that parses the flags, and nothing ties the two together. They agree today
because I checked, which is maintenance rather than a guarantee: add a flag, forget
the string, and they disagree with nothing to say so.
`tests/cli.rs::help_and_version` asserts only that `--help` prints something
containing `usage:`. The test that would close this — pull the option names out of
`USAGE`, assert the parser accepts each one and rejects nothing listed — is not
written, and it should be.

What I got back is error text that says what was expected *here* — `--format takes
'human' or 'json', not 'jsonn'` — rather than reprinting a grammar.

### `anyhow` — 909,556,524 all-time · 200,860,233 in 90 days
### `thiserror` — 1,377,720,340 all-time · 338,962,699 in 90 days
[`src/error.rs`](src/error.rs). One enum, four variants — `Syntax`, `InFile`, `Io`,
`Usage` — hand-written `Display` and `std::error::Error` with a real `source()`.

**What I gave up:** `thiserror`'s derive, which for four variants is about
fifteen lines of boilerplate — a fair trade. `anyhow`'s backtrace capture and its
`.context()` chaining, which I miss more; adding a file path to an IO error is
manual here. What I got is that `Error::Syntax` carries `line` and `col` as
structured fields rather than a formatted string, so the tests can assert on
position without parsing the message back out.

---

### `toml` — 855,052,855 all-time · 201,425,545 in 90 days
[`src/toml.rs`](src/toml.rs). A documented subset, sufficient for `Cargo.lock`,
`poetry.lock` and `uv.lock` — three formats for one parser, though only two
ecosystems: poetry and uv are both PyPI.

Supported: `key = value`, `[table]`, `[dotted.table]`, `[[array.of.tables]]`,
basic strings with the full escape set including `\uXXXX` and `\UXXXXXXXX`,
literal strings, multi-line strings with the line-ending fold, decimal integers
with `_` separators, booleans, multi-line arrays with trailing commas, single-line
inline tables, comments.

**What I gave up:** floats, dates, times, date-times, hex/octal/binary integers,
dotted keys outside a header, and multi-line inline tables. Every one of those is
*refused with a line and column* rather than mis-parsed, which was the design rule
— a parser that guesses at a construct it does not know produces a plausible wrong
answer, and a plausible wrong answer in a security tool is worse than an error.

The subset is only sufficient because `uv.lock` stores timestamps as strings
(`upload-time = "2026-03-26T01:21:00.379Z"`). If it stored them as TOML datetimes
this parser would refuse the file, loudly. The only bare integers in all six
fixtures are `version` and `revision`.

Two things the real files taught that guessing would have missed. poetry writes
**quoted keys containing dots** — `"jaraco.classes" = "*"` — so quoting, not the
dot, decides key-versus-path; treating that as a dotted key silently invents a
`jaraco` table. And there are **no triple-quoted strings anywhere** in any of the
six fixtures, contrary to what I assumed going in. They are implemented anyway,
because mis-reading one is worse than refusing it, but nothing in the corpus
exercises them.

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
[`src/semver.rs`](src/semver.rs). Version parsing and semver precedence. That is
the whole module, and the drift rule is what uses it: when one package name is
installed at several versions, the report has to put them in an order, and byte
order is not that order — it sorts `2.10.0` below `2.9.0` and `1.0.0-rc.1` above
`1.0.0`.

Precedence is where implementations go wrong, and there are three separate ways to
do it. `1.0.0-beta.11 > 1.0.0-beta.2` needs numeric segments compared as numbers
rather than strings. `1.0.0-1 < 1.0.0-alpha` needs numeric to sort *below*
alphanumeric. And `1.0.0-rc.1 < 1.0.0` needs the empty prerelease list to be the
largest rather than the smallest, which inverts the usual intuition about empty.
`tests/semver.rs` runs the exact ordering table from semver.org section 11, both
pairwise and as a sort. Build metadata is dropped at parse time rather than stored
and then carefully ignored at every comparison.

**What I gave up:** range matching — and the honest part is that I wrote it first
and then deleted it. There was a `Req` type: caret, tilde, the four comparison
operators, and the awkward `^0.x` / `^0.0.x` cases npm and Cargo agree on. It had
tests and the tests passed. It went anyway, for two reasons.

Nothing in `stranger` resolves a range. The tool reads lockfiles, and a lockfile is
the output of a resolver — the version is already pinned on the line in front of
you. `Req` answered a question this program never asks.

And it was wrong in two places its passing tests never reached. `~1` came out as
`>=1.0.0, <1.1.0` where npm and Cargo both say `<2.0.0`, because the tilde bound
always bumped the minor regardless of how many components were given. And every
operator matched prereleases it should have excluded: `^1.2.3` accepted
`2.0.0-alpha`, since `2.0.0-alpha` is genuinely below the `2.0.0` upper bound and
the rule that a range without a prerelease does not match a version with one was
never written down. Both are the ordinary failure mode of code with no caller —
the tests only ask what the author thought to ask.

Untested-in-anger code that answers nobody's question is decoration, and this file
is about what was traded rather than what was typed. So: no `>=1.2, <1.5`, no
`1.2.x || 2.x`, and nothing that reads a `package.json` range. If a rule ever needs
"how far apart are these two versions", `Req` comes back out of git history with
those two bugs fixed, rather than sitting in the binary being nearly right.

### `itoa` — 1,265,455,201 all-time · 292,076,583 in 90 days
[`src/report.rs`](src/report.rs), `thousands()`. `core::fmt::NumBuffer` plus
`format_into`, stable in 1.98, writes digits into a stack buffer with no
allocation and no `Display` machinery.

**What I gave up:** nothing measurable. The 1.98 toolchain killed this one for
free; the standard library benchmarks `format_into` on par with `itoa`. Listing
it because it is an honest substitution, not because it was hard.

---

### `walkdir` — 588,096,443 all-time · 138,629,879 in 90 days
### `glob` — 575,867,559 all-time · 119,415,458 in 90 days
[`src/walk.rs`](src/walk.rs). An explicit stack over `std::fs::read_dir`, with a
skip list, a depth bound, and sorted output.

**What I gave up:** `walkdir`'s symlink-loop detection (mine simply does not
follow symlinks, which is cruder and sufficient), its parallel bridge, and
`glob`'s pattern language — there are no patterns here, only a list of filenames
matched by suffix.

Two things are not concessions but the reason it exists. `walkdir` would happily
descend into `node_modules`, where a populated tree holds hundreds of other
people's vendored lockfiles; the skip list is the whole point. And `read_dir`
returns filesystem order, which on ext4 is hash order, so results are sorted —
otherwise two scans of the same tree differ only in sequence and a diff between
them is noise.

## State and concurrency

### `once_cell` — 1,187,857,958 all-time · 262,379,838 in 90 days
[`src/corpus.rs`](src/corpus.rs). `std::sync::LazyLock`, stable since 1.80. The
three corpora load through one each.

**What I gave up:** nothing. `once_cell::sync::Lazy` and `LazyLock` are the same
thing, and people still reach for the crate out of habit. Another one the
toolchain killed for free.

### `rayon` — 516,056,344 all-time · 116,467,391 in 90 days
### `crossbeam-channel` — 565,386,665 all-time · 110,054,999 in 90 days
[`src/main.rs`](src/main.rs), `scan_all`. `std::thread::scope` and
`std::sync::mpsc`.

A directory scan is several independent lockfiles, and the slow part of each is
the corpus search — pure CPU over a shared read-only slice. `thread::scope` is
what makes that safe without `Arc`: the closures *borrow* the path slice rather
than cloning into each thread, because the scope guarantees every thread is
joined before it returns.

**What I gave up:** `rayon`'s work-stealing scheduler and its parallel iterators.
This spawns one thread per lockfile, which is wrong for four hundred files and
right for the single digits the walk actually produces — `node_modules` is
skipped, so a real repo has a handful. The comment on it names the upgrade:
chunk across `available_parallelism()`. From `crossbeam-channel` I gave
up `select!` and the multi-consumer end; one producer per thread into one
consumer is all this needs.

Results come back in path order rather than completion order, because two runs
over one tree have to produce the same bytes or a diff between scans is noise.
`tests/cli.rs::a_directory_scan_is_deterministic` runs the same scan five times
and compares.

### `rand` — 1,605,926,795 all-time · 401,565,502 in 90 days
[`tests/distance.rs`](tests/distance.rs), [`tests/ablation.rs`](tests/ablation.rs)
and [`tests/fuzz.rs`](tests/fuzz.rs). Five lines of xorshift, written out three
times because the three call sites want different things from the seed. The
property tests seed from `SystemTime` nanoseconds and print it so a failing case
replays; the ablation and the fuzzer seed from a constant so their published
results reproduce. All three are the same xorshift64, the same shifts, because two
variants in one repository would be two things to reason about for no gain.

**What I gave up:** a great deal, and it does not matter here. xorshift64 is not
cryptographically secure, has a far shorter period than ChaCha, and fails some
statistical tests `rand`'s generators pass. Its three jobs are generating random
short strings for property tests, thinning a corpus deterministically, and picking
byte offsets to corrupt. None of them cares. If anything in `stranger` ever needed
randomness for a security decision this would be the wrong tool — but nothing does,
because the tool has no secrets and makes no nonces.

Seed 0 is the one value that breaks it, since xorshift is all zeroes forever from
there — a fuzz run from seed 0 would corrupt byte 0 five thousand times and pass.

The guard used to be `| 1` everywhere, which avoids zero and pays half the seed
space for it: every even seed folds onto its odd neighbour, so seeds 2 and 3 were
the same run. That was costing the fuzzer real coverage — two of its extra seeds
were one seed counted twice — so it and the property tests substitute a constant
for zero and leave every other seed alone.

The corpus-thinning ablation still uses `| 1`, and that asymmetry is deliberate
rather than an oversight left behind. It has one published constant seed, so there
is no space to halve, and the guard is part of what the published table
reproduces against: `SEED` is even, so dropping it thins a different tenth of the
corpus. Changing it by accident renumbered every row — 126,004 names kept became
126,019, and the 90% false-positive count went from 36 to 72. The comment on that
line says so, because "make it consistent" is exactly the tidy-up that would break
it next time.

Worth stating plainly since the hackathon rules forbid rolling your own crypto:
this is not crypto and is not used as crypto. `stranger` computes no hashes,
verifies no signatures, and has no key material.

---

---

### `serde_yaml` — 383,697,832 all-time · 88,928,764 in 90 days
[`src/yaml.rs`](src/yaml.rs). A subset sufficient for `pnpm-lock.yaml` v9.

Accepted: block mappings nested by indentation, block sequences, plain scalars,
single- and double-quoted scalars, single-line flow collections, comments, a
leading BOM, CRLF.

**What I gave up:** anchors, aliases, tags, directives, block scalars,
multi-line quoted scalars, multiple documents, and mappings inside sequence
items. Each is refused *by name at the indicator* rather than by falling through
— the plain-scalar scanner would otherwise read `&anchor 1` as the string
`"&anchor 1"` and be silently, plausibly wrong.

The decision worth defending is implicit typing. YAML 1.1 turns `no`, `on`,
`off` and `y` into booleans, which is the famous Norway problem. Exactly two
tokens are typed here — lowercase `true` and lowercase `false` — and everything
else stays a string, because **`no`, `on`, `y` and `off` are all registered npm
package names**. A reader that turned the key `no@1.0.0` into a boolean would
drop a package out of a supply-chain audit without a word. `no` and `on` are in `corpus/npm.txt`; `y` and `off` are registered on npm and are not, because the corpus is the top 140,066 names by download count and not the whole registry. Which is the same distinction the tool makes about `Origin::Elsewhere`: absence from a popularity sample is not evidence a package does not exist. `null`, `~`, `Yes`,
`010` and `1e3` all stay strings for the same reason.

## Terminal

### `owo-colors` — 156,700,441 all-time · 32,842,569 in 90 days
### `comfy-table` — 94,630,283 all-time · 17,750,081 in 90 days
### `is-terminal` — 324,499,410 all-time · 56,521,071 in 90 days
[`src/term.rs`](src/term.rs), 138 lines for all three.

`is-terminal` is the interesting one. The traditional way to ask whether stdout is
a terminal is an FFI call to `libc::isatty`, which needs an `unsafe` block —
impossible under `#![forbid(unsafe_code)]`. `std::io::IsTerminal` has been stable
since 1.70 and does it in safe code. That entire crate is now one line of std.

`NO_COLOR`, `CLICOLOR_FORCE`, `--no-color` and the TTY check are resolved in one
function that takes the environment as arguments rather than reading it, because
`std::env::set_var` is `unsafe` in edition 2024 and a test that mutates the
environment is therefore not writable in this crate at all.

**What I gave up (`owo-colors`, `comfy-table`):** typed style combinators and
supports-color detection; borders, spanning, wrapping and alignment. What is here
computes column widths from content and pads. That is all the report needs.

**What I gave up (`is-terminal`):** the ability to fix it myself. The crate carries
workarounds for the cases that are actually hard — MSYS and Cygwin pseudo-terminals
on Windows, where the handle is a named pipe and is a terminal anyway — and it can
ship a correction on its own release schedule. A wrong answer inside `std` waits for
a Rust release and there is nothing I can patch in the meantime. I also gave up a
low minimum compiler version: the crate predates the std API, so leaning on
`IsTerminal` puts a 1.70 floor under the project. That costs nothing here, because
`substr_range` and `format_into` already put the floor at 1.98, but it is the real
cost anywhere the floor matters.

Worth stating next to the loss: the alternative is not a slightly worse one-liner.
It is `#![forbid(unsafe_code)]` coming off the crate root, because one `unsafe`
block for `isatty` disables the lint for every file in the crate. At that price the
Windows edge cases are cheap, particularly since `stranger` is developed and tested
on Linux, where they do not arise.

The width measurement is `chars().count()`, which is wrong for East Asian
wide forms, combining marks and emoji ZWJ sequences. Correcting it means shipping
a table generated from `EastAsianWidth.txt` and tracking a Unicode version, to
align package names that npm, PyPI and crates.io all restrict to ASCII. The
comment on it names that upgrade path.

---

## Not claimed

This section listed `serde_yaml`, `indicatif`, `rayon` and `crossbeam-channel` as
entries that would land if and only if their modules did. Three of the four
landed — `yaml.rs` with the pnpm reader, and `std::thread::scope` plus
`std::sync::mpsc` with the parallel scan — and they are written up above.

**`indicatif` is not claimed**, because nothing here draws a progress bar. The
largest fixture scans in under half a second, and a bar that finishes before it
renders is a dependency bought for a frame. Claiming it would have meant writing a
spinner in order to say a spinner crate had been replaced, which is the shape of
padding rather than substitution.

The rule this section exists to enforce: an unwritten module gets no entry.

---

## Data that is not code, disclosed anyway

`corpus/` holds 160,066 package names across three ecosystems, fetched once with
`curl` at development time on 2026-08-28. `fixtures/` holds sixteen real
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
