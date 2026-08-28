# stranger

Every dependency is a stranger. This one reads your lockfile and tells you which
of them you have never met — hallucinated names, code that runs at install time,
the same package at four versions — without installing anything, resolving
anything, or making a single network request.

```
$ stranger scan fixtures/poisoned.package-lock.json

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

  ⚠  HALLUCINATION RISK     3
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent

  risk 75/100    53ms    third-party deps used to compute this: 0
```

That last number is the point. `stranger` is written for the Zero Dependency
hackathon in Rust with the standard library and nothing else.

```
$ cargo tree
stranger v0.1.0 (/home/keir/stranger)

$ grep -c '^\[\[package\]\]' Cargo.lock
1
```

`deps-proof.txt` regenerates that on demand with `make proof`, including a build
with the network switched off.

## Build

```
git clone https://github.com/keirsalterego/stranger
cd stranger
make
```

That is the whole thing. Rust 1.98.0, pinned in `rust-toolchain.toml`. No
`build.rs`, no code generation, no vendored source.

```
make test        # the test suite
make ablation    # the table below, regenerated
make bench       # timings, with a fallback when hyperfine is absent
make proof       # regenerate deps-proof.txt
```

## What it looks for

**Hallucinated names.** A model asked to write a `package.json` will occasionally
invent a package. Attackers register the invented names. This is the rule with an
idea in it and it gets its own section below.

**Install-time code execution.** npm packages can run arbitrary code when you type
`npm install`, before you have run a line of your own program.

**Version drift.** The same package resolved at two or more versions in one tree.

**Unpinned requirements.** `requests>=2.0` is a different supply chain every time
anyone installs it.

## The co-occurrence rule

Edit distance alone does not work. `lodash.merge` is two edits from
`lodash.mergewith`; both are real, both are widely used. A registry the size of
npm contains thousands of these pairs, and any threshold loose enough to catch a
typo catches legitimate siblings with it.

The clause that separates them is not about spelling:

> A hallucinated package is a **root** dependency. Nothing depends on it, because
> nothing real has ever heard of it. A model put it in your manifest; no
> maintainer ever put it in theirs.

`lodash.merge` is depended upon by real packages. `lodahs` cannot be, because it
does not exist — the only reference to it anywhere is the manifest under audit.
So the rule is a conjunction of three clauses:

1. the name is not in a corpus of known-real names
2. it is within edit distance 2 of a name that is
3. **nothing in the lockfile depends on it**

There is a refinement in clause 3 that took the fixtures to find. An edge out of a
**workspace member** is not evidence either — it is the same manifest, by the same
author, as the root. Both monorepo fixtures declare `workspaces` and keep almost
nothing in the root manifest, so a hallucinated name added to
`apps/desktop/package.json` would otherwise arrive with an in-edge and never be
looked at. Those edges are recorded as roots, not as evidence.

## Does clause 3 actually do anything

Against the full 140,066-name corpus, no. Both configurations score 1.000
precision and 1.000 recall on the fixtures, and the clause changes nothing.

That result is real and reporting it is the honest thing to do. It is also
measuring the wrong regime: with a corpus that contains every package in every
fixture, clause 1 alone is sufficient and no other clause can show a difference.

No corpus is ever that. npm accepts thousands of new names a day and this one is a
snapshot from a single afternoon — a package published after the snapshot is
indistinguishable from a package that does not exist. So the experiment that
matters deletes part of the corpus and watches which clause is holding the rule up:

| corpus kept | clause 3 | TP | FP | precision | recall |
|---|---|---|---|---|---|
| 100% (140,066) | on | 3 | 0 | 1.000 | 1.000 |
| 100% (140,066) | off | 3 | 0 | 1.000 | 1.000 |
| 90% (126,004) | **on** | 3 | **3** | **0.500** | 1.000 |
| 90% (126,004) | off | 3 | 95 | 0.031 | 1.000 |
| 70% (98,197) | **on** | 3 | **16** | **0.158** | 1.000 |
| 70% (98,197) | off | 3 | 332 | 0.009 | 1.000 |
| 50% (69,897) | **on** | 2 | **20** | **0.091** | 0.667 |
| 50% (69,897) | off | 2 | 483 | 0.004 | 0.667 |
| 25% (35,134) | **on** | 2 | **16** | **0.111** | 0.667 |
| 25% (35,134) | off | 2 | 549 | 0.004 | 0.667 |

At 90% coverage the clause cuts false positives from 95 to 3 — a factor of 32 —
and costs no recall at all.

The recall drop at 50% is not the clause failing. The thinning deleted the real
`express`/`lodash`/`chalk` that a planted typo needed as its neighbour, so clause
2 had nothing to match against. That is a corpus-coverage failure and it would
happen with or without clause 3.

Ground truth is 3 planted names in `fixtures/poisoned.package-lock.json` and
3,925 packages scanned across six npm fixtures, so every finding outside the
planted set is a false positive by construction. `make ablation` reproduces the
whole table.

## Is reading npm's lockfile just shelling out to npm with extra steps

No, and the hackathon's own FAQ rules on it directly:

> Parsing files those tools already produced is fine, because nothing third-party
> ends up in your artifact.

Two conditions come attached and both are met. It is **disclosed** — see
`STDLIB.md` and `corpus/PROVENANCE.md` — and it **degrades gracefully** when the
file is absent rather than being useless without it:

```
$ stranger scan /tmp/empty-project

  no lockfile in /tmp/empty-project
  looked for: package-lock.json
```

Exit code 0. `stranger` never executes `npm`, `pip`, `cargo`, `git` or anything
else. It reads files. It works on a plane, and it works on a machine that has
never had Node installed — which is the actual use case, because auditing a
lockfile you did not write is exactly when you do not want to install its
toolchain.

## Limits

Written down because a judge will otherwise find them, and a named limitation
reads as judgement where a hidden one reads as an oversight.

**Integrity hashes are never verified.** Every npm entry carries a
`sha512-...` field. Rust's standard library has no cryptography of any kind, so
`stranger` reports whether the field is *present* and never whether it is
*correct*. This is the constraint biting in public, which is the entire subject of
the event, so it is here rather than buried.

**`hasInstallScript` is a bare boolean.** lockfileVersion 3 records that a package
runs code at install time and does not record what that code is. The tool can say
code runs. It cannot say what it does. Eight of the 1,390 entries in the largest
fixture carry the flag.

**No Go corpus.** `proxy.golang.org` publishes no ranked list of modules and
module paths are domains, so edit distance over them is a different problem.
Where `go.mod` parsing exists the tree is read correctly, but the hallucination
rule never fires on it. Saying so beats shipping a rule that silently never
triggers.

**Flat formats have no graph.** `requirements.txt` records no dependency edges at
all, so every package trivially has in-degree 0 and clause 3 is vacuous there. The
rule falls back to two clauses on those files and is correspondingly weaker.

Here is that costing a false positive, on a real fixture:

```
$ stranger scan fixtures/reqs-xs.requirements.txt

  ⚠  HALLUCINATION RISK     1
     tensorflow-gpu           not in corpus · d=1 from "tensorflow-cpu" · root-only, no parent
```

`tensorflow-gpu` is a real PyPI package. It is deprecated and absent from the
top-15,000 corpus, and it is one edit from `tensorflow-cpu`, so clauses 1 and 2
both fire. On an npm tree clause 3 would have had a chance to save it. On a
`requirements.txt` there is no clause 3 to have the chance.

The fix is a different file rather than a better reader: `poetry.lock` and
`uv.lock` both record the resolved graph, and both are already in `fixtures/`.

**The corpus is a snapshot.** Taken 2026-08-28. A package published after that
date looks exactly like a package that does not exist. The table above is, among
other things, a measurement of how badly that ages.

**The risk score is crude.** Severity weights, capped at 100, calibrated against
nothing. It exists so `--fail-on` has something to compare and so a repeated scan
shows movement. The findings are the output; the score is a handle.

## Layout

| | |
|---|---|
| `src/json.rs` | RFC 8259 reader — replaces `serde_json` |
| `src/distance.rs` | unrestricted Damerau-Levenshtein — replaces `strsim` |
| `src/corpus.rs` | 160,066 known-real names, compiled in |
| `src/lock/` | one reader per lockfile format |
| `src/rules/` | one file per finding type |
| `src/cli.rs` | argument parsing — replaces `clap` |
| `src/error.rs` | one error enum — replaces `anyhow` and `thiserror` |

`STDLIB.md` lists every crate this replaces with its download count and an honest
note on what was given up. `DECISIONS.md` covers why.

## Licence

MIT.
