# JSON conformance

`stranger` nominates [`serde_json`](https://crates.io/crates/serde_json) for the
Package Killer bonus — 1,227,048,507 all-time downloads, 288,758,389 in the last
ninety days, measured 2026-08-28. Almost every Rust program that reads a
`package-lock.json` reaches for it.

The case against reaching for it here is `src/json.rs`, and "I wrote a JSON
parser" is not that case. Anyone can write one that reads the happy path. Two
things are on this page instead, and both are commands you can run.

## Clause by clause

```console
$ cargo test --test json_conformance
```

29 tests, each citing the RFC 8259 section it comes from. The grammar has few
enough productions to walk exhaustively, so it is walked:

| §2 | the six structural characters, whitespace being exactly four characters |
| §3 | the literal names, lowercase and whole — `True` and `nul` are errors |
| §4 | objects, member names, trailing commas, and that a duplicate name takes the last |
| §5 | arrays, trailing and missing commas |
| §6 | leading zeros, leading `+`, a decimal point with digits on both sides, the exponent forms, and the numbers Rust's own float parser would have taken but JSON does not — `inf`, `NaN`, `1.`, `.5` |
| §7 | all **eight** two-character escapes, `\uXXXX`, surrogate pairs, unescaped control characters, unterminated strings |
| §8 | a leading byte order mark, and lone or mispaired surrogates |
| §9 | one value and no more |

Eight escapes, not six: `"` `\` `/` `b` `f` `n` `r` `t`. This repository said six
in two places until somebody counted.

## Against a reference implementation

```console
$ ./scripts/json-differential.sh
```

Two million generated and mutated inputs, fed to this parser and to CPython's
`json`, comparing both the accept/reject decision and the parsed value.

The oracle is configured with `parse_constant` so it rejects `NaN`, `Infinity`
and `-Infinity`. Python accepts all three by default and RFC 8259 has none of
them, so leaving that alone would have been comparing against Python's dialect
rather than against the grammar.

**1,997,016 agreed. 2,984 disagreed, in four classes.** None of the
disagreements was about a *value*: every time both accepted, both built the same
thing, down to the IEEE-754 bits.

| n | class | who is right |
|---|---|---|
| 1,093 | a leading BOM — we skip it, CPython raises | **neither.** §8.1 says a parser MAY ignore one |
| 898 | lone high surrogate, `"\ud83e"` | **neither.** §8.2 does not forbid unpaired surrogates |
| 825 | lone low surrogate, `"\udd80"` | same |
| 168 | high surrogate then a non-surrogate escape | same |

Every class is a place the RFC permits both answers, which is the honest reading
and is less satisfying than "we were right four times". Both choices here are
still choices, and both were made for a reason:

**The BOM is skipped** because a `package-lock.json` saved by a Windows editor
starts with one, and the useful behaviour is to audit that file rather than to
refuse it.

**Unpaired surrogates are rejected** because a Rust `String` cannot hold one.
Accepting means either WTF-8 or substituting U+FFFD — and silently rewriting a
character of a package name is exactly the bug class this tool exists to find. A
corrupt name should stop the scan, not quietly become a different name. That is
the same reasoning as [`term::sanitize`](../limits.md), pointed the other way.

## What the campaign did not find

No defect in `src/json.rs`. That is a result reported rather than a claim made,
and it was checked past the edges the campaign reaches: a 100,000-digit integer,
a 100,000-digit exponent, a `\u` escape truncated at every offset, one whose
fourth byte lands mid-codepoint, the depth cap at 127, 128 and 129, and error
columns counted in characters behind multi-byte text. All correct.

`tests/fuzz.rs` adds 1,356,800 mutants and 169,834 truncation prefixes across
every parser and all seven readers, with no panic anywhere.

## Python is dev-time tooling

`python3` runs the oracle and nothing else. It never enters `Cargo.toml`, it is
not linked, it does not ship, and the binary a judge builds has no idea it
exists. The 29 clause tests run on a machine that does not have it; the
differential run says so in a sentence and exits clean.

That is the same standing as the `curl` that fetched the name corpus, and it is
disclosed the same way — in `STDLIB.md`, in `corpus/PROVENANCE.md`, and here.
