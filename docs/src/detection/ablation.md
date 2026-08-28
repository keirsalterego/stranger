# The ablation table

The third clause is the claim, so it gets measured rather than asserted.

Ground truth is the fixture set. `poisoned.package-lock.json` contains exactly
three planted names — `expres`, `lodahs`, `chalck` — and the other five npm
fixtures contain none, so any finding outside the planted set is a false positive
by construction. 3,925 packages across six files.

```console
$ make ablation
```

## Against the full corpus, the clause is worth nothing

| in-degree clause | TP | FP | FN | precision | recall |
|---|---|---|---|---|---|
| on (shipped) | 3 | 0 | 0 | 1.000 | 1.000 |
| off (ablated) | 3 | 0 | 0 | 1.000 | 1.000 |

Identical. Both configurations find all three planted names and flag nothing else
across 3,925 packages.

That is a real result and it belongs at the top of this page rather than buried
at the bottom. It also measures the wrong thing. The 140,066-name corpus contains
every package in every fixture, so clause 1 alone is sufficient and no other
clause can possibly show a difference. Perfect scores here say the corpus is
good, not that the rule is.

## No corpus is ever complete

npm accepts thousands of new names a day. This corpus is a snapshot taken on one
afternoon — 2026-08-28, one `curl` run, written up in `corpus/PROVENANCE.md`. A
package published the day after that snapshot is, to clause 1, indistinguishable
from a package that does not exist.

So the question worth measuring is what happens as clause 1 degrades. Delete a
fraction of the corpus and watch which clause is still holding the rule up. The
thinning is a seeded xorshift, so the table is reproducible rather than different
every run.

| corpus kept | in-degree clause | TP | FP | precision | recall |
|---|---|---|---|---|---|
| 100% (140066) | on | 3 | 0 | 1.000 | 1.000 |
| 100% (140066) | off | 3 | 0 | 1.000 | 1.000 |
| 90% (126004) | on | 3 | 3 | 0.500 | 1.000 |
| 90% (126004) | off | 3 | 95 | 0.031 | 1.000 |
| 70% (98197) | on | 3 | 16 | 0.158 | 1.000 |
| 70% (98197) | off | 3 | 332 | 0.009 | 1.000 |
| 50% (69897) | on | 2 | 20 | 0.091 | 0.667 |
| 50% (69897) | off | 2 | 483 | 0.004 | 0.667 |
| 25% (35134) | on | 2 | 16 | 0.111 | 0.667 |
| 25% (35134) | off | 2 | 549 | 0.004 | 0.667 |

Read the 90% row first, because it is the realistic one. Ten percent of the
corpus missing is roughly what a few months of registry growth looks like. The
clause takes false positives from 95 down to 3 — a 32-fold cut — and recall stays
at 1.000. Nothing was traded for it.

At 70% the ratio is 332 to 16, about 21-fold. At 25% it is 549 to 16, about
34-fold. The clause never makes precision worse at any level, and it never costs
a true positive; the ablation test asserts both of those rather than leaving them
to the reader's eye.

## The verdict outlives the explanation

Something more interesting than the headline number falls out of the decay run,
and it took reproducing the thinning by hand to see.

The thinning is deterministic — seed `0x5EED1234`, the same xorshift, over
`corpus/npm.txt` in order — so you can ask exactly which names it deleted. At 90%
all three parents survive. At 70% `express` is gone. At 50% and 25% `lodash` is
gone too, and only `chalk` is left.

Now look at what the tool reports at 70%. `express` has been deleted, and `expres`
is still flagged — matching `espree` at distance 2. At 50% and 25% it matches
`rxpress`. The finding is still correct, and its stated reason is not.

```text
70%   expres -> espree   (d=2)
50%   expres -> rxpress  (d=2)
25%   expres -> rxpress  (d=2)
```

So the two halves of a finding decay at different rates. **The verdict — this
name has no evidence behind it — survives corpus loss much better than the
explanation — this name is a typo of that one.** By the time clause 1 has lost
30% of its coverage, the "nearest real name" printed in `detail` is a name the
typo has nothing to do with.

The practical reading: treat `d=1 from "chalk"` as the rule showing its working,
not as an identification. It is the closest surviving corpus entry, which is only
the actual parent when the corpus still contains the actual parent.

## The recall drop is not the clause

Recall falls from 1.000 to 0.667 at 50% and stays there at 25%, in **both**
columns. That is worth being clear about, because a careless reading blames the
in-degree clause for it.

It is clause 2 failing, not clause 3. Thinning at 50% deleted `lodash` itself.
`lodahs` is still absent from the corpus and still has no parent, but there is no
longer a real name within distance 2 for it to be a typo *of*, so clause 2 finds
nothing and the rule stays quiet. Losing a planted name's parent loses the planted
name.

`expres` kept firing through the same loss because a coincidental neighbour
existed. `lodahs` had none. Which of the two happens is luck about the shape of
the registry, not a property of the rule.

That is a corpus-coverage failure. It shows up identically whether the in-degree
clause is on or off, which is exactly what you would expect from a failure that
has nothing to do with it. A tool whose corpus is half gone has bigger problems
than which clause is enabled.

## Running it

The decay table scans the fixtures ten times against a 140,000-name corpus and
takes about two minutes, so it is `#[ignore]`d and gets its own target:

```console
$ make ablation
cargo test --release --test ablation -- --nocapture --include-ignored
...
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 109.42s
```

CI runs it on every push, because the README quotes its numbers. The fast table
runs on every `cargo test`.

```console
$ cargo test --test ablation -- --nocapture
```
