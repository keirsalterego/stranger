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
     tensorflow-gpu           not in corpus · d=1 from "tensorflow-cpu" · root-only, no parent

  ⚠  UNPINNED               12    no exact version recorded

  risk 77/100    14ms    third-party deps used to compute this: 0
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

## Flat formats lose the clause that would have saved it

The `tensorflow-gpu` finding is the pip case specifically. `requirements.txt`
records no dependency edges, so every package has in-degree 0, clause 3 eliminates
nothing, and the rule runs on two clauses instead of three.

On an npm tree the same package would have had a chance: something real depends on
`tensorflow-gpu`, and that edge would have suppressed the finding. On a flat file
there is no edge to find.

The fix is a different file rather than a better reader. `poetry.lock` and
`uv.lock` both record the resolved graph and both are already in `fixtures/`;
neither has a reader yet.

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
