# Decision log

The argued version lives at the repository root, in
[DECISIONS.md](https://github.com/keirsalterego/stranger/blob/main/DECISIONS.md).
This page is the index, so the two cannot drift into disagreeing with each other:
if a decision is described in both places, the root file is the one that is right.

## What is in DECISIONS.md

| section | the question it answers |
|---|---|
| One crate, not a workspace | why the layout is flat |
| Skipping the Single File bonus, on purpose | what was traded for readability |
| The corpus is data, and it is compiled in | why 2.9 MB of names is in the binary |
| Why the in-degree clause exists | the reasoning behind the third clause |
| The refinement the fixtures forced | why a workspace member's edges are not evidence |
| Measuring the idea instead of asserting it | why there is an ablation at all |
| The TOML subset | what is accepted, what is refused, and why |
| What the xorshift is for | deterministic thinning, and the RNG std does not ship |
| Reading files that other tools produced | the FAQ ruling and its two conditions |

It ends with a **Defence** section — nine questions written as if by a hostile
reviewer, each answered against the code:

- Walk me through how your JSON parser handles a lone high surrogate.
- Why Damerau and not plain Levenshtein? Show me a case where it matters.
- Which clause carries the most signal, and how do you know?
- Where does this tool give a wrong answer, and what would you do about it with a
  week?
- Why zero unsafe — was that hard, or free?
- You read `integrity` fields but never check them. Why?
- Isn't reading npm's lockfile just shelling out to npm with extra steps?

Plus a **Cuts** section: what was deliberately not built.

## What is in STDLIB.md

The crate-by-crate accounting of what the standard library replaced, with download
counts and an honest note on what was given up in each case:
`serde_json`, `clap`, `anyhow`, `thiserror`, `toml`, `strsim`, `semver`, `itoa`,
`walkdir`, `glob`, `once_cell`, `rand`, `owo-colors`, `comfy-table`,
`is-terminal`.

It also carries the disclosure of data that is not code — the corpus and the
fixture lockfiles — which is one of the two conditions attached to the FAQ ruling
that lets this tool read lockfiles at all.

## What is in this book instead

Decisions that are about *using* the tool rather than about building it live on
the pages they affect, because that is where somebody meets them:

| decision | page |
|---|---|
| why exit 2 is separate from exit 1 | [Exit codes](../using/exit-codes.md) |
| why non-critical rules collapse to a count | [Your first scan](../using/first-scan.md) |
| the colour precedence order | [Your first scan](../using/first-scan.md) |
| why clause order is 1, 3, 2 | [The co-occurrence rule](../detection/rule.md) |
| why distance 2 and not 3 | [The co-occurrence rule](../detection/rule.md) |
| why each rule has the severity it has | its own page under Other rules |
| why versions are compared for equality only | [Version drift](../rules/drift.md) |
| why the trivial list is hand-written | [Trivial packages](../rules/trivial.md) |
| why `--index-url` is dropped | [pip](../formats/pip.md) |
| why discovery does not recurse | [Limits](../limits.md) |
| the three reproducible-build settings | [Reproducible builds](reproducible-builds.md) |

```console
$ ./target/release/stranger scan --format yaml fixtures/poisoned.package-lock.json; echo $?
```
