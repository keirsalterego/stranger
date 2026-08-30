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

  ⚠  INSTALL SCRIPTS        3     arbitrary code at install time

  ⚠  TRIVIAL                35    (4.6% of third-party)

  ⚠  VERSION DRIFT          55    same package at 2+ versions in one tree

  ·  UNPINNED               — no signal in this format

  risk 81/100    56ms    third-party deps used to compute this: 0
```

Critical findings get their lines. The rest are a count and what the count means,
until you ask with `-v`.

That last number is the point. `stranger` is written for the Zero Dependency
hackathon in Rust with the standard library and nothing else.

```
$ cargo tree
stranger v0.1.0 (/home/keir/stranger)

$ grep -c '^\[\[package\]\]' Cargo.lock
1
```

It will also make that claim about itself, in its own output, through the same
reader it points at anybody else's project:

```
$ stranger scan Cargo.lock

  Cargo.lock               0 packages   (0 direct · 0 transitive · 1 workspace)

  no findings
  ·  INSTALL SCRIPTS        — no signal in this format
  ·  UNPINNED               — no signal in this format

  risk 0/100    0ms    third-party deps used to compute this: 0
```

The one workspace entry is `stranger`. Three different questions — the resolver's,
the lockfile's, and the auditor's — and they agree. `deps-proof.txt` regenerates
all of it on demand with `make proof`, including a build with the network switched
off.

### GitHub says this repository has 22 vulnerabilities

It does, and every one of them is in `fixtures/`. Nineteen are in
`reqs-s.requirements.txt`, a real `requirements.txt` taken off disk with an old
Pillow in it, and three are in `poisoned.requirements.txt`, which is poisoned on
purpose. None are in `stranger`, because `stranger` has no dependencies to have
vulnerabilities in.

That is worth leaving switched on rather than silencing. A supply-chain auditor
whose own test data is a pile of vulnerable packages is working as intended: the
fixtures have to be real files with real problems or the tool is being tested
against a fantasy. `.github/dependabot.yml` already turns off the *update* half,
because Dependabot opened a pull request against `poisoned.requirements.txt`
within an hour of this repository going public and would have bumped
`python-dateutils==2.9.0` — a package that does not exist — quietly breaking the
demo it was planted for. The alerts stay because they are true.

The number to compare it against is the one in the scan output above: the
dependencies used to *compute* the audit, which is zero.

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

## Point it at a repository

```
$ stranger scan .
```

The walk finds every lockfile under the directory and audits them on separate
threads — seven formats and four ecosystems in one pass, if that is what is there.
It does not descend into `node_modules`, `target`, `.venv` or `dist`, because a
populated `node_modules` holds hundreds of vendored lockfiles belonging to other
people and auditing those is worse than auditing nothing.

Results come out in path order rather than whichever thread finished first. Two
runs over one tree produce the same bytes, so a diff between scans is a diff.

There is a second subcommand, and it answers the question the first one raises:

```
$ stranger tree <pkg> [path]
```

`scan` says a package has no parent. `tree` shows you. It is the same walk over
the same lockfiles, printing who depends on one name, how many of them there
are, and what that name depends on. [Looking at clause 3](#looking-at-clause-3)
is what it looks like on a planted name.

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

Edit distance alone does not work. `lodash.assign` is two edits from
`lodash.assignin` — exactly the shipped `MAX_EDIT_DISTANCE` — and both names are
in `corpus/npm.txt`, so both are real. A registry the size of npm contains
thousands of these pairs, and any threshold loose enough to catch a typo catches
legitimate siblings with it.

The clause that separates them is not about spelling:

> A hallucinated package is a **root** dependency. Nothing depends on it, because
> nothing real has ever heard of it. A model put it in your manifest; no
> maintainer ever put it in theirs.

`lodash.assign` is depended upon by real packages. `lodahs` is not, and cannot be:
whatever briefly lived at that name, nothing ever built on it. The only reference
to it in this tree is the manifest under audit.

That sentence used to read "because it does not exist", which is a stronger claim
than the rule makes and than the registry supports —
[checked](fixtures/README.md#what-the-registries-actually-say), `lodahs` resolves
on npm today as `0.0.1-security`, npm's own holding package for a name taken down
by its security team. That is not a weaker fact. It is npm confirming that a real
typosquat of `lodash` by that exact spelling existed and was removed, which is the
best corroboration the rule could ask for. The clause was never about registration;
it is about who depends on it.

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

## Looking at clause 3

The first two clauses are checkable by hand. A name is in `corpus/npm.txt` or it
is not, and an edit distance is arithmetic. Clause 3 is a claim about a graph,
and until you can see the graph the only thing to do with it is believe the
report. `stranger tree` prints the graph around one name:

```
$ stranger tree lodahs fixtures/

  fixtures/poisoned.package-lock.json   npm · 757 packages

  lodahs@4.17.21   node_modules/lodahs

     depended on by   in-degree 0 · root-only, no parent
                      nothing in this lockfile depends on it. The only
                      reference to the name in the file is the manifest under
                      audit. That is clause 3 of the co-occurrence rule: a
                      hallucinated package is a root dependency, because
                      nothing real has ever heard of it.

     depends on       nothing
```

The same command on a package that exists gives the other answer, out of the
same reader and the same 754-package tree:

```
$ stranger tree accepts fixtures/npm-l.package-lock.json

  fixtures/npm-l.package-lock.json   npm · 754 packages

  accepts@2.0.0   node_modules/accepts
     dev-only

     depended on by   in-degree 1
                      express@5.2.1

     depends on       2 direct, to depth 3
     ├─ mime-types@3.0.2
     │  └─ mime-db@1.54.0
     └─ negotiator@1.0.0
```

`--depth` moves the cut and `--depth 0` removes it. A lockfile is a graph rather
than a tree, so a name whose dependencies were printed earlier comes back marked
`(*)` and a cycle prints as `· cycle` — neither is followed twice, and neither is
dropped silently. A name at several versions is several blocks, because picking
one would hide the drift finding a scan of the same file raises. A name that is
not there exits 0 and lists what is close.

## Does clause 3 actually do anything

Against the full npm list, no. These fixtures are all npm, so the list in play is
the 140,066 npm names rather than the whole 160,066-name corpus. Both
configurations score 1.000 precision and 1.000 recall on the fixtures, and the
clause changes nothing.

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

At 90% coverage the clause cuts false positives from 95 to 3 — a factor of 31.7 —
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
$ rm -rf /tmp/empty-project && mkdir -p /tmp/empty-project
$ stranger scan /tmp/empty-project

  no lockfile stranger reads in /tmp/empty-project
  looked for: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock, go.mod
```

Exit code 0. `stranger` never executes `npm`, `pip`, `cargo`, `git` or anything
else. It reads files. It works on a plane, and it works on a machine that has
never had Node installed — which is the actual use case, because auditing a
lockfile you did not write is exactly when you do not want to install its
toolchain.

## Reproducible build

The claim is not that some particular hash is eternal. It is that two builds of
whatever commit you have checked out come out byte-identical. `make repro` builds
it twice and prints what it got:

```
$ make repro
commit:  8c57728e2dd9e75a0b0cd375074688eb8d3d3525
rustc:   rustc 1.98.0 (88d9e12ae 2026-08-18)
epoch:   1787940000

build A  /tmp/stranger-repro.22490/a
         b10155b3cf626c37fcdaa8347e686c1d0e6a0fdcd859fce7215908f2d418ab0f
build B  /tmp/stranger-repro.22490/b-with-a-deliberately-longer-name
         b10155b3cf626c37fcdaa8347e686c1d0e6a0fdcd859fce7215908f2d418ab0f

MATCH — byte-identical across two directories
```

That is a real run, at the commit it names, on the rustc it names. The line
carrying the claim is `MATCH`. The hash is a function of the checkout and the
toolchain: it moves whenever `src/` or `Cargo.lock` moves, and a different rustc
gives a different one. So do not diff it against a hash written down in a file —
run `make repro` on the commit in front of you and read the last line. CI runs
`scripts/repro.sh` on every pull request and every push to `main`, which is what
stops a commit from silently ceasing to reproduce.

Two directories rather than one, with deliberately different path lengths, because
the absolute build path is the thing most likely to leak into a binary — it ends
up in panic messages. Three settings do the work:

```
SOURCE_DATE_EPOCH=1787940000 CARGO_INCREMENTAL=0 \
RUSTFLAGS="--remap-path-prefix=$PWD=/build -C debuginfo=0" \
cargo build --release --locked
```

`SOURCE_DATE_EPOCH` is the hackathon kickoff. `CARGO_INCREMENTAL=0` because
incremental artifacts are not deterministic. The remap is what makes two
directories produce one binary.

## Package Killer: `serde_json`

1,227,048,507 all-time downloads and 288,758,389 in the last ninety days, measured
2026-08-28. Almost every Rust program that reads a `package-lock.json` reaches for
it, and [`src/json.rs`](src/json.rs) is why this one does not.

"I wrote a JSON parser" is not a case for anything — anyone can write one that
reads the happy path. The case is two commands:

```console
$ cargo test --test json_conformance
$ ./scripts/json-differential.sh
```

The first is 29 tests, each citing the RFC 8259 section it comes from, walking the
grammar production by production. The second generates and mutates two million
inputs and puts this parser next to CPython's `json`, comparing the accept/reject
decision and the parsed value. **1,997,016 agreed.** The 2,984 that did not fall
into four classes, and every one of them is a place the RFC permits both answers —
a leading byte order mark and three flavours of unpaired surrogate. None was about
a value: every time both accepted, both built the same thing down to the IEEE-754
bits.

The campaign found no defect in `src/json.rs`. That is a result reported rather
than a claim made, and the four disagreements are enumerated with the reasoning
for each choice in
[`docs/src/reference/json-conformance.md`](docs/src/reference/json-conformance.md).

`python3` runs the oracle and nothing else. It never enters `Cargo.toml`, is not
linked, does not ship, and the 29 clause tests run on a machine that does not have
it. The other seventeen substitutions, with their download counts and what each
one cost, are in [STDLIB.md](STDLIB.md).

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
code runs. It cannot say what it does.

Nine of the 1,391 keys in the largest fixture's `packages` map carry the flag
and the tool reports eight. The ninth is the empty-string key — the root project —
and its install script is your own build, not a stranger's. Dropping that key is
also why 1,391 keys are 1,390 entries.

Those 1,390 then print as `1,376 packages … 14 workspace`, because the header
counts third-party packages and sets your own code aside. Three numbers for one
file, all of them right, and this paragraph names all three because a reader who
sees 1,391 here and 1,376 on their terminal should not have to work out which of
us is wrong.

**`Cargo.lock` records no build scripts and no dev-dependencies.** Cargo runs
`build.rs` at compile time — the same shape `hasInstallScript` flags — and the
lockfile says nothing about which crates have one. It also does not mark
dev-dependencies or optional ones. So `install_script`, `dev` and `optional` are
`false` on every crate, and that is a blank rather than a measurement. Guessing
from the name (`-sys`, say) would turn a blank into a confident wrong answer.

**pnpm 9 dropped the install-script field; pnpm 6 has it.** `requiresBuild`
exists at lockfileVersion 6 and not at 9, so `stranger` reports install scripts
on a v6 file and says *no signal in this format* on a v9 one. Both are
`pnpm-lock.yaml`, which is why the reader answers that question and the filename
does not. `hasBin: true` appears 42 times in the v9 fixture and is not a
substitute — it means the package ships a `bin` to symlink, not that code runs
at install time, and mapping one to the other would put 42 invented High
findings in the report. The newer file is the one that can hide a build step.

**No install-script signal on Python at all.** Neither `poetry.lock` nor
`uv.lock` records whether a package runs code at install time, and an sdist with
no wheel is suggestive rather than the same claim. So `install_script` is `false`
on every Python entry and the scripts rule never fires on one. A `setup.py` that
phones home is invisible to this tool.

**poetry does not record which dependencies are direct.** They live in
`pyproject.toml`, which is not the lockfile. `roots` is derived instead — an
entry nothing else depends on can only have arrived through the root's manifest —
and that derivation is wrong in one direction: a direct dependency that something
else also needs has an in-edge and drops out of the count. `uv.lock` quantifies
the error, because it records the root explicitly: 91 real direct dependencies
against the 60 the derivation would have found. So `poetry-m`'s reported 75
direct is a floor, not a count.

**No Go corpus.** `go.mod` reads. The `gomod-m` fixture is 174 requirements, 50
direct against 124 `// indirect`, and that split — the only graph the format
has — is read correctly.

What does not read is the name. `proxy.golang.org` publishes no ranked list of
modules and a module path is a domain, so edit distance over them is a different
problem and there is nothing for a name to be absent from. `corpus::names`
returns an empty slice for `Ecosystem::Go`, and the hallucination rule stops on
its first line rather than running three clauses against a list nobody
publishes. That check is on the ecosystem's own corpus and not on the one the
ablation passes in, so no configuration turns it back on: `tests/gomod.rs` hands
the rule a one-edit neighbour of a real module in the tree and it still says
nothing.

So a Go scan reports nothing: three rules because the format records nothing
they read, the trivial-package rule because its list is npm micro-packages, and
this one by decision. What you get is the tree, the direct and transitive
counts, and the package list under `--format json`. Saying that plainly beats
shipping a rule that silently never triggers.

**A path that is not valid UTF-8 is refused, not scanned.** On Linux a filename
is arbitrary bytes. `std::env::args()` *panics* on one that is not UTF-8 — it
took the process down at exit 134, before a line of this crate ran — so argv is
read with `args_os` and a non-UTF-8 argument is a usage error with a message.
Refused rather than handled: making it work end to end means `OsStr` through the
walker, the readers and the report, which is a real change for a case no lockfile
in the wild has produced. Named here rather than left as a surprise.

**Flat formats have no graph.** `requirements.txt` and `go.mod` record no
dependency edges at all, so every package trivially has in-degree 0 and clause 3
is vacuous there. The rule falls back to two clauses on those files and is
correspondingly weaker — on `requirements.txt`, at least, which is the only one
of the two where the rule runs at all. `stranger tree` will not print an
in-degree on either of them: a zero nobody measured is the same mistake in a
different place.

Here is that costing a false positive, on a real fixture:

```
$ stranger scan fixtures/reqs-xs.requirements.txt

  ⚠  HALLUCINATION RISK     1
     tensorflow-gpu           not in corpus · d=1 from "tensorflow-cpu" · no dependency graph in this format
```

`tensorflow-gpu` is a real PyPI package. It is deprecated and absent from the
top-15,000 corpus, and it is one edit from `tensorflow-cpu`, so clauses 1 and 2
both fire. On an npm tree clause 3 would have had a chance to save it. On a
`requirements.txt` there is no clause 3 to have the chance.

The fix is a different file rather than a better reader, and it has landed:
`poetry.lock` and `uv.lock` both record the resolved graph, and `stranger` now
reads both. Point it at one of those instead and clause 3 has something to work
with — 283 edges across 233 packages in `poetry-m.poetry.lock`, 476 across 250
in `uv-m.uv.lock`.

Thin the corpus, which is the honest way to ask what a clause is worth, and the
asymmetry is the whole argument:

| corpus kept | candidates clause 3 removes on `poetry-m` | on `uv-m` | on any `requirements.txt` |
|---|---|---|---|
| 90% | 2 of 3 | 2 of 4 | **0** |
| 70% | 6 of 10 | 12 of 15 | **0** |
| 50% | 5 of 11 | 8 of 10 | **0** |
| 25% | 8 of 13 | 6 of 8 | **0** |

Between 45.5% and 80.0% of the candidates on a file with a graph, and exactly
zero on a flat file at every corpus size, because there is no edge in the file to
read.

The candidate counts here are about a third of what this table said before the
length budget landed, and that is the budget working rather than the clause
weakening: most of what a thinned corpus used to hand clause 3 were short names
sitting near something by arithmetic, and those no longer reach clause 3 at all.
The *share* clause 3 removes barely moved.
The thinning is the same seeded xorshift `tests/ablation.rs` uses, seed
`0x5EED_1234`, so these rows are reproducible rather than different every run.
`tests/pypi.rs` pins the asymmetry itself.

**A corpus can only speak about one registry.** A package pulled from git, a
private index or a direct URL never passed through the public registry the corpus
samples, so its absence from the list is not evidence of anything — the list was
never asked. The Cargo reader found this the hard way: `slint` and `sg` in the
`cargo-m` fixture are real crates fetched from git, and all three clauses fired on
both. Packages are now tagged with an origin and the name rules stay quiet unless
the lockfile says the package came from the registry.

What that does *not* fix is a real registry package outside the corpus.

Until the last day this section named three false positives — `ksni` in
`cargo-m.Cargo.lock`, `taze` in `pnpm-l.pnpm-lock.yaml`, and `tensorflow-gpu` —
and called the first two unlucky near-misses of the popularity cut. **They were
not unlucky. They were structural, and the argument for leaving them in was
wrong.**

`ksni` and `taze` are both four characters long. Measure what that means: take
every name in a corpus, pretend it is missing — which is exactly what a real
package below the popularity cut looks like — and ask whether the rest of the
list offers it a neighbour within two edits. On npm the answer at four
characters is **100% of the time**. At three characters it is 100% on all three
registries. So for a short name, clause 2 was not a filter at all, it was a
formality, and the rule collapsed to "not in the corpus AND in-degree zero" —
which is a guaranteed CRITICAL for any real package that happens to be
unpopular.

The fix is a budget that scales with length: one edit per five characters, capped
at two. The five came from that same table — on npm, a hit at distance 1 stops
being the likelier outcome at five characters and a hit at distance 2 at ten
characters — and from a sweep of nine candidate policies against every fixture.
Both tables are in the doc comment on `distance::CHARS_PER_EDIT`, and both are
tests you can run.

| policy | true positives | false positives | recall | precision |
|---|---|---|---|---|
| flat threshold of 2 — what shipped for most of the weekend | 7 | 5 | 1.000 | 0.583 |
| **`min(2, len / 5)` — what ships now** | **7** | **1** | **1.000** | **0.875** |
| a tighter flat threshold of 1 | 6 | 4 | 0.857 | 0.600 |

Every planted name still fires, at the same distance and against the same parent.
`ksni`, `taze`, and two short names in the hostile fixture stop firing. The last
row is there because "just lower the threshold" is the obvious alternative and it
is worse at both ends: it loses `requests-http`, a true positive at two edits from
`requests-html`, and it still keeps four false positives.

**One false positive is left, and it is the honest one.** `tensorflow-gpu` is
fourteen characters and one edit from `tensorflow-cpu`. No length policy reaches
it, because at fourteen characters a near-miss really is evidence. It is a real,
deprecated PyPI package that fell out of a top-15,000 list, it came through the
registry the corpus samples so the origin check has nothing to say about it, and
it sits in a `requirements.txt` where there is no clause 3 to save it. It stays in
the fixtures.

That is corpus coverage, and it is the next limit.

**The corpus is a snapshot.** Taken 2026-08-28. A package published after that
date looks exactly like a package that does not exist. The table above is, among
other things, a measurement of how badly that ages.

**The risk score is crude.** A band for the worst severity present — critical 75,
high 50, medium 25, low 1 — plus a saturating term for how many findings share it,
calibrated against nothing. Two projects are comparable at the band and not below
it. `--fail-on` compares severities and never reads this number; gate on that.
The findings are the output; the score is a handle.

## Layout

| | |
|---|---|
| `src/json.rs` | RFC 8259 reader — replaces `serde_json` |
| `src/distance.rs` | unrestricted Damerau-Levenshtein — replaces `strsim` |
| `src/corpus.rs` | 160,066 known-real names, compiled in |
| `src/lock/` | one reader per lockfile format |
| `src/rules/` | one file per finding type |
| `src/tree.rs` | `stranger tree` — the graph around one name |
| `src/cli.rs` | argument parsing — replaces `clap` |
| `src/error.rs` | one error enum — replaces `anyhow` and `thiserror` |

`STDLIB.md` lists every crate this replaces with its download count and an honest
note on what was given up. `DECISIONS.md` covers why.

## Licence

MIT.
