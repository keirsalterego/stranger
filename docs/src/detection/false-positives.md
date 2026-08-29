# False positives

On the npm fixture set with the full corpus, there are none. 3,925 packages
across six lockfiles, three findings, and all three are the planted names.

That number is a property of the corpus and of the npm format. Here is where it
breaks.

## A real package published after the snapshot

This is the failure mode, and it is not hypothetical — the repository ships a
fixture that triggers it.

```console
$ ./target/release/stranger scan fixtures/reqs-xs.requirements.txt

  reqs-xs.requirements.txt 12 packages   (12 direct · 0 transitive)

  ⚠  HALLUCINATION RISK     1
     tensorflow-gpu           not in corpus · d=1 from "tensorflow-cpu" · no dependency graph in this format

  ⚠  UNPINNED               12    no exact version recorded

  ·  INSTALL SCRIPTS        — no signal in this format

  risk 77/100    4ms    third-party deps used to compute this: 0
```

`tensorflow-gpu` is a real PyPI package. It is deprecated, which is why it is
absent from a top-15,000 corpus, and it is one edit from `tensorflow-cpu`, which
is present. Clauses 1 and 2 both fire. There is no clause 3 on a flat file to
stop them.

You can check both halves yourself:

```console
$ grep -x -E 'tensorflow(-gpu|-cpu)?' corpus/pypi.txt
tensorflow
tensorflow-cpu
```

Clause 1 asks whether a name is in a list fetched on 2026-08-28. npm accepts
thousands of new names a day and PyPI is no slower. A package published after
that date, or one that fell off a popularity ranking before it, fails clause 1
for a reason that has nothing to do with being fake.

The [ablation table](ablation.md) puts a number on the npm side: delete 10% of
the corpus, roughly what a few months of registry growth costs you, and the false
positive count goes from 0 to 3. Delete 30% and it goes to 16.

## Short names, and the two false positives that are gone

Two findings were on this page until the last day of the window: `ksni` in
`cargo-m.Cargo.lock` and `taze` in `pnpm-l.pnpm-lock.yaml`. Both were described as
real packages that had fallen below a popularity cut — bad luck, and the corpus's
fault.

That was the wrong diagnosis. Both names are **four characters long**, and length
turns out to be the variable that matters.

The measurement is leave-one-out over each corpus: take a real name, pretend it is
missing — which is exactly what a real package below the cut looks like to clause
1 — and ask whether the rest of the list offers it a neighbour. That is the false
positive rate, as a function of length:

| chars | npm k=1 | npm k=2 | pypi k=1 | pypi k=2 | crates k=1 | crates k=2 |
|---|---|---|---|---|---|---|
| 2 | 99.6% | 100.0% | 88.0% | 100.0% | 36.4% | 100.0% |
| 3 | 98.6% | 100.0% | 69.9% | 100.0% | 60.4% | 100.0% |
| 4 | 51.9% | **100.0%** | 43.9% | 98.9% | 41.4% | 99.1% |
| 5 | 40.5% | 97.5% | 34.0% | 93.8% | 18.7% | 78.9% |
| 6 | 36.5% | 85.8% | 16.3% | 76.6% | 12.5% | 51.5% |
| 8 | 30.0% | 63.0% | 11.3% | 35.6% | 7.0% | 26.6% |
| 9 | 27.7% | 55.1% | 5.9% | 23.1% | 4.1% | 19.3% |
| 10 | 18.8% | **46.1%** | 2.1% | 14.9% | 4.8% | 9.5% |

At four characters, a name absent from npm finds a neighbour within two edits
**every single time**. So clause 2 was not weighing evidence about `ksni` or
`taze`; it was passing everything, and the rule was really running on two clauses:
"not in the corpus" and "in-degree zero". For a real package that nobody depends
on — a devDependency of the root manifest, say — that is a guaranteed CRITICAL.

The threshold is a function of length now: one edit per five characters, capped at
two, which is `distance::budget_for`. Five is where the table points. Reading the
npm column, a hit at k = 1 stops being the likelier outcome at five characters and
a hit at k = 2 at ten characters — below a coin flip being the bar, because a
clause that fires on most inputs is not evidence about any of them. That is one
edit per five characters, twice.

Nine policies were swept against every fixture, with the seven planted names as
ground truth and everything else counted as a false positive:

| policy | TP | FP | recall | precision |
|---|---|---|---|---|
| `2` — a flat threshold, what shipped | 7 | 5 | 1.000 | 0.583 |
| `min(2, len / 3)` | 7 | 4 | 1.000 | 0.636 |
| `min(2, (len - 1) / 3)` | 7 | 3 | 1.000 | 0.700 |
| `min(2, len / 4)` | 7 | 3 | 1.000 | 0.700 |
| **`min(2, len / 5)` — ships** | **7** | **1** | **1.000** | **0.875** |
| `1` — a tighter flat threshold instead | 6 | 4 | 0.857 | 0.600 |

Recall does not move. All seven planted names still fire, at the same distances
and against the same parents.

Four policies tie at 0.875, so the fixtures do not pick between them — the
leave-one-out table does. `(len - 1) / 4` hands out two edits at nine characters,
where npm still answers 55% of the time. `(len - 1) / 5` and `len / 6` refuse
`nunpy` its edit at five characters, and `nunpy` is a true positive `tests/pip.rs`
already holds the rule to.

The last row is there because "just lower the threshold" is the obvious
alternative, and it is worse at both ends: it loses `requests-http` — a genuine
hallucination two edits from `requests-html` — and still keeps four false
positives.

Both tables are in the doc comment on `distance::CHARS_PER_EDIT`, and both are
tests: `tests/corpus.rs::length_is_the_false_positive_rate` (ignored by default,
about six minutes) and `tests/ablation.rs::edit_budget_policy_sweep`.

**What it does not fix** is the next section. `tensorflow-gpu` is fourteen
characters, and at fourteen characters a near-miss really is evidence — no length
policy reaches it, and it should not.

## Flat formats lose the clause that would have saved it

The `tensorflow-gpu` finding is the pip case specifically. `requirements.txt`
records no dependency edges, so every package has in-degree 0, clause 3 eliminates
nothing, and the rule runs on two clauses instead of three.

On an npm tree the same package would have had a chance: something real depends on
`tensorflow-gpu`, and that edge would have suppressed the finding. On a flat file
there is no edge to find.

The fix is a different file rather than a better reader, and both of those files
read today. [`poetry.lock` and `uv.lock`](../formats/poetry-uv.md) record the
resolved graph, so a Python project that keeps one of them gets three clauses
where a `requirements.txt` project gets two. `poetry-m` scans as 233 packages,
75 direct and 158 transitive; `uv-m` as 249, 91 direct and 158 transitive. Those
transitive counts are clause 3's raw material, and on `reqs-xs` above the same
column reads 0.

This is not retroactive relief for `tensorflow-gpu`. That fixture is a
`requirements.txt` and stays a two-clause scan; the point is that the format is
the thing to change, not the reader.

## The nearest name can be wrong even when the verdict is right

`detail` names the closest corpus entry within distance 2. When the corpus is
complete that is nearly always the typo's actual parent. When it is not, the tool
still picks the closest surviving name and prints it with the same confidence. In
the 70% ablation row, `expres` is reported against `espree` rather than `express`,
because `express` had been deleted.

Treat `d=1 from "chalk"` as the rule showing its working, not as an
identification.

## A brand-new direct dependency

The worst case for the rule is the case it is designed for, seen from the other
side: you have added a genuinely new, genuinely real package to your manifest. It
is a root dependency, nothing depends on it, and it is too recent for the corpus.
All three clauses fire.

There is no way for the tool to tell that apart from a hallucination, because from
inside the lockfile there is no difference. The evidence that would settle it —
does this name exist on the registry — is on the network, and the tool does not go
there.

The right reading of a finding is "no evidence this name is real", not "this name
is fake".

## The trivial rule is wrong more often than it is right

`slopsquat` gets the careful treatment because it is the rule with an idea in it.
The noisiest rule is [trivial](../rules/trivial.md), and it says so in its own
documentation: its second clause looks for a predicate-shaped name that resolves
no dependencies, and has no way to know how long the file behind that name is.

`is-callable` is dozens of lines of edge cases around one `typeof`. `is-docker`
reads `/proc` and memoises the answer. Both are reported. Neither is a one-liner.
That is not an occasional miss — it is a good share of what the clause finds on a
real tree, which is why the rule is `low` and collapses to a count by default.

## What it misses

Two categories, and they matter more than the false positives.

**A typosquat that actually got registered.** The corpus is a list of names that
exist on npm and PyPI, harvested from npm and PyPI. It is not a list of names that
are safe. If an attacker registered `lodahs` this morning, it would be in a corpus
rebuilt this afternoon, clause 1 would pass, and the rule would never fire. The
corpus makes the tool quiet about real names; whether a real name is malicious is
a different question and this tool does not ask it.

**A hallucinated name that is genuinely close to nothing.** Clause 2 needs a
neighbour within two edits. A name like `api-client-utils` has none and stays
silent. Note that this net is wider than it looks — `requests-http` was planted as
an example of exactly this and turned out to be two edits from the real
`requests-html`, so the rule caught it after all.

## What it deliberately ignores

Workspace members and `link: true` entries are first-party and skipped before any
clause runs, by every rule. In `npm-xl` that is 14 of 1,390 entries. Without the
exclusion every monorepo scan is mostly noise about the project scanning itself.

## Checking a finding

You cannot do it from the lockfile — that is the whole point. Open the registry
page for the name. If it does not exist, you have your answer. If it exists but
was published last week by an account with no history, you have a different and
more interesting answer.

```console
$ ./target/release/stranger scan --format json fixtures/reqs-xs.requirements.txt | jq -r '.findings[] | select(.rule=="slopsquat") | .package'
```
