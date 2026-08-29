# Decisions

Why the things are the way they are, written as they were decided rather than
reconstructed afterwards.

The repository also carries
[DECISIONS.md](https://github.com/keirsalterego/stranger/blob/main/DECISIONS.md),
which is the submission's own copy and includes a written defence section. Where
the two overlap, the root file is canonical — this page is the readable one, and
the table at the bottom says which decisions live on other pages of this book
instead.

## One crate, not a workspace

`stranger` is twenty-five source files in one crate. It could be `stranger-json`,
`stranger-toml`, `stranger-lock` and so on, and that would look more serious.

A workspace here would be an abstraction with one consumer. None of these modules
are separately useful, none are separately versioned, and splitting them buys a
longer build and a `Cargo.lock` with more `[[package]]` blocks in it. For a
project whose central claim is that the file contains exactly one, that is an
actively bad trade.

Modules give the same separation. `cargo build` already tells me if `json.rs`
broke `npm.rs`.

## Skipping the Single File bonus

There is a bonus for shipping as one file. Twenty-five files crushed into one
`main.rs` trades a 25% criterion for a 5% bonus, and the 25% one is code quality
judged by somebody who will not enjoy scrolling past a JSON parser to reach an
argument parser.

Declining it with a reason reads as judgement. Declining it silently reads as an
oversight.

## The corpus is data, and it is compiled in

160,066 package names ship inside the binary through `include_str!` — 2,960,053
bytes of text in a 4,064,792-byte release binary. Nearly three quarters of the
binary is corpus.

The alternative, fetching at runtime or reading a cache directory, would have made
the central claim false. `stranger` works on a plane. There is no *corpus not
found* failure mode, no first-run download, no stale-cache logic, and no code path
where a network timeout changes the answer. Three megabytes deletes an entire
category of failure.

Rust's standard library has no TLS, so a network request here is not merely
against policy, it is unavailable. That is worth stating as a design property
rather than apologising for as a limitation.

## Why the in-degree clause exists

Edit distance is not a rule. `http-proxy-agent` is one edit from
`https-proxy-agent`; both are real, both are in the corpus, and both are depended
on by other packages in `npm-xl`. So are `safe-buffer` and `safer-buffer`, also one
edit apart. Take just the 1,077 distinct names the npm fixtures install: between
them they have 9,453 neighbours within distance 2 in the corpus, every one a
package that exists. A threshold loose enough to catch a typo catches legitimate
siblings, and precision collapses.

The observation that separates them is not about spelling:

> A hallucinated package is a **root** dependency. Nothing depends on it, because
> nothing real has ever heard of it. A model put it in your manifest; no
> maintainer ever put it in theirs.

The only reference to `lodahs` — one transposition from `lodash` — anywhere in the
world is the manifest being audited. Real packages, including the boring near-miss
siblings, are depended upon by other packages.

So the rule is a conjunction: not in corpus, **and** within edit distance 2,
**and** in-degree zero. [The co-occurrence rule](detection/rule.md) has the
detail, including why the clauses are evaluated 1, 3, 2.

### The refinement the fixtures forced

The first version counted every dependency edge as evidence. Both monorepo
fixtures then reported *zero* direct dependencies for projects of 582 and 1,390
lockfile entries, because both declare `workspaces` and keep almost nothing in the
root manifest.

Fixing that by "also read the workspace members" would have been wrong in an
interesting way. An edge out of a workspace member is the same manifest, by the
same author, as the root. If a model wrote `apps/desktop/package.json`, a
hallucinated name in it arrives with an in-edge and clause 3 never looks at it.

Workspace-member edges are recorded as roots, not as evidence. Same author, same
absence of independent confirmation. [Monorepos](cookbook/monorepos.md) covers
what that looks like in use.

## Measuring the idea instead of asserting it

Against the full corpus the clause is worth exactly nothing: 1.000 precision and
1.000 recall with it and without it. That result is at the *top* of
[the ablation table](detection/ablation.md), because a measurement that undercuts
my own idea is the one most worth publishing.

It measures nothing because the corpus contains every package in every fixture, so
clause 1 alone suffices and nothing else can show a difference. No real corpus has
that property. So the experiment thins the corpus and re-measures: at 90%
coverage, the clause takes false positives from 95 to 3 at no cost to recall.

Making the corpus a parameter of the rule rather than a global is what made that
measurable. An assumption you cannot vary is one you cannot measure.

## A corpus can only speak about one registry

The Cargo reader landed and immediately produced three findings on
`cargo-m.Cargo.lock`. Two were wrong in a way worth fixing rather than
documenting: `slint` and `sg` are real crates fetched straight from git. They
never went through crates.io, so a crates.io corpus cannot have heard of them, and
only workspace members reference them, so nothing depends on them either. All
three clauses fire on a package that is entirely legitimate.

The bug was treating "absent from a list" as evidence when the list was never
asked. Packages now carry an origin, and the name rules stay quiet unless the
lockfile says the package came from the registry the corpus samples.

What that does *not* fix is a real registry package outside the corpus.
[False positives](detection/false-positives.md) has the three live ones.

## The TOML subset

One parser reads `Cargo.lock`, `poetry.lock` and `uv.lock` — three formats for
the price of one, which bought more per line written than anything after JSON.

Accepted: `key = value`, `[table]` and `[dotted.table]` headers,
`[[array.of.tables]]`, basic strings with the full escape set including `\uXXXX`
and `\UXXXXXXXX`, literal strings, multi-line strings with the line-ending
backslash fold, decimal integers with `_` separators, booleans, arrays over as
many lines as they like with a trailing comma allowed, single-line inline tables,
and `#` comments.

Refused, each with a line and column: floats, dates, times and date-times as bare
values; hex, octal and binary integers; dotted keys outside a table header;
inline tables spread over several lines, which is TOML 1.1; duplicate keys; and a
`[table]` header that reopens a table already defined.

Refusing beats guessing. A parser that improvises at a construct it does not know
produces a plausible wrong answer, and a plausible wrong answer in a security tool
is worse than an error.

Three things the real files taught that guessing would have missed:

- **The subset is only sufficient because `uv.lock` stores timestamps as
  strings** — `upload-time = "2026-03-26T01:21:00.379Z"`. If it wrote them as
  TOML datetimes, this parser would refuse the file, loudly. The only bare
  integers in all six fixtures are `version` and `revision`: 1, 3 and 4.
- **poetry writes quoted keys containing dots**: `"jaraco.classes" = "*"`. That
  is one key whose name contains a dot, not a dotted key. Quoting decides, not
  the dot — conflating them silently invents a `jaraco` table.
- **No triple-quoted string appears anywhere in the corpus.** Not one, across six
  real lockfiles, contrary to what I assumed going in. They are implemented
  because a lockfile is allowed to contain one and mis-reading it would be worse
  than refusing it, but nothing here exercises them.

## Two booleans, and everything else stays a string

YAML 1.1's implicit typing turns `no`, `on`, `off` and `y` into booleans. That is
the Norway problem, and in a supply-chain tool it is not a curiosity.

`no`, `on`, `y` and `off` are all registered npm package names. A reader that
turned the key `no@1.0.0` into a boolean would drop a package out of an audit
without saying anything. `no` and `on` are in `corpus/npm.txt`; `y` and `off` are registered on npm and are not, because the corpus is the top 140,066 names by download count and not the whole registry. Which is the same distinction the tool makes about `Origin::Elsewhere`: absence from a popularity sample is not evidence a package does not exist. So `yaml.rs` types exactly two tokens — lowercase `true` and lowercase
`false`, which the fixture genuinely needs for `hasBin` and `optional` — and
leaves `null`, `~`, `Yes`, `010` and `1e3` as strings.

## What the xorshift is for

Rust's standard library has no random number generator, and two things need one:
property tests over random short strings, and deterministic corpus thinning for
the ablation.

Five lines of xorshift64\*, seeded from `SystemTime` nanoseconds in the property
tests — and printed, so a failure replays — and from a fixed constant in the
ablation, so the published table reproduces.

It has a short period and fails statistical tests a real generator passes. That is
fine for four-letter strings out of a four-character alphabet. It would not be
fine for anything security-sensitive, and nothing here is: `stranger` computes no
hashes, verifies no signatures, holds no key material and makes no nonces. The
rules forbid rolling your own crypto, and this is not crypto.

## Reading files that other tools produced

The hackathon rules forbid shelling out to an installed tool, and the FAQ rules
this design in explicitly:

> Parsing files those tools already produced is fine, because nothing third-party
> ends up in your artifact.

Two conditions attach and both are honoured. It is disclosed, in `STDLIB.md` and
`corpus/PROVENANCE.md`. And it degrades gracefully: a directory with no lockfile
prints what it looked for and exits 0.

`stranger` never executes another program. Not `npm`, not `pip`, not `cargo`, not
`git`. There is no `std::process::Command` anywhere in `src/`, and CI fails the
build if one appears. [A project whose toolchain you do not
have](cookbook/no-toolchain.md) demonstrates the point.

## Decisions that live on other pages

Decisions about *using* the tool live where somebody meets them:

| decision | page |
|---|---|
| why exit 2 is separate from exit 1 | [Exit codes](using/exit-codes.md) |
| why non-critical rules collapse to a count | [Your first scan](using/first-scan.md) |
| the colour precedence order | [Your first scan](using/first-scan.md) |
| why clause order is 1, 3, 2 | [The co-occurrence rule](detection/rule.md) |
| why distance 2 and not 3 | [The co-occurrence rule](detection/rule.md) |
| why each rule has the severity it has | its own page under the other four rules |
| why versions are compared for equality only | [Version drift](rules/drift.md) |
| why the trivial list is hand-written | [Trivial packages](rules/trivial.md) |
| why `--index-url` is dropped | [pip](formats/pip.md) |
| the three reproducible-build settings | [Reproducible builds](reference/reproducible-builds.md) |

## What was traded away

The crate-by-crate accounting of what the standard library replaced —
`serde_json`, `clap`, `toml`, `serde_yaml`, `semver`, `strsim`, `walkdir`,
`glob`, `owo-colors`, `comfy-table`, `is-terminal`, `rand`, `rayon`,
`crossbeam-channel`, `itoa`, `once_cell`, `anyhow` and `thiserror` — is in
[STDLIB.md](https://github.com/keirsalterego/stranger/blob/main/STDLIB.md), with
download counts and an honest note on what each substitution gave up.

It also carries the disclosure of data that is not code: the name corpus and the
fixture lockfiles. That disclosure is one of the two conditions attached to the
FAQ ruling above.

## Check the ones that are checkable

Several decisions on this page are assertions a reader can test rather than take
on trust. These are the commands, and they need no network:

```console
$ grep -rn 'Command::new' src/ | wc -l
0

$ cargo tree | wc -l
1

$ grep -c '^\[\[package\]\]' Cargo.lock
1

$ make ablation
```

The last one re-derives the corpus-decay table, which takes about two minutes.
