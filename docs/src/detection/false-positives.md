# False positives

On the fixture set with the full corpus, there are none. 3,925 packages across
six npm lockfiles, three findings, and all three are the planted names.

That number is a property of the corpus, not of the rule. Here is where it
breaks.

## A real package published after the snapshot

This is the failure mode, and everything else on this page is a variation of it.

Clause 1 asks whether a name is in a list of 140,066 npm names fetched on
2026-08-28. npm accepts thousands of new names a day. A package published on
2026-08-29 fails clause 1 for a reason that has nothing to do with it being
fake, and if it happens to be within two edits of an older name and nothing
depends on it yet — which is normal for a new package — it gets reported.

The [ablation table](ablation.md) puts a number on it. Delete 10% of the corpus,
roughly what a few months of registry growth costs you, and the false positive
count goes from 0 to 3. Delete 30% and it goes to 16.

Two things reduce this and neither eliminates it. The corpus is deliberately
long-tailed rather than popular-only — 126,702 of its names come from a
676-query registry sweep, precisely so that obscure-but-real packages are not
flagged. And clause 3 suppresses anything a third-party package vouches for,
which is most of a dependency tree.

## A brand-new direct dependency

The worst case for the rule is the case it is designed for, seen from the other
side: you have added a genuinely new, genuinely real package to your
manifest. It is a root dependency, nothing depends on it, and it is too recent
for the corpus. All three clauses fire.

There is no way for the tool to tell that apart from a hallucination, because
from inside the lockfile there is no difference. The evidence that would settle
it — does this name exist on the registry — is on the network, and the tool does
not go there.

The right reading of a finding is "no evidence this name is real", not "this
name is fake".

## The nearest name can be wrong even when the verdict is right

`detail` names the closest corpus entry within distance 2. When the corpus is
complete that is nearly always the typo's actual parent. When it is not, the
tool still picks the closest surviving name and prints it with the same
confidence. In the 70% ablation row, `expres` is reported against `espree`
rather than `express`, because `express` had been deleted.

Treat `d=1 from "chalk"` as the rule showing its work, not as an identification.

## What it misses

Two categories, and they matter more than the false positives.

**A typosquat that actually got registered.** The corpus is a list of names that
exist on npm, harvested from npm. It is not a list of names that are safe. If an
attacker registered `lodahs` this morning, it would be in a corpus rebuilt this
afternoon, clause 1 would pass, and the rule would never fire. The corpus makes
the tool quiet about real names; whether a real name is malicious is a different
question and this tool does not ask it.

**A hallucinated name that is not a near-miss of anything.** Models invent
plausible-sounding names, not only typos — `requests-http`, `api-client-utils`,
that shape. Clause 2 finds nothing within distance 2 and the rule stays silent.
`fixtures/poisoned.requirements.txt` carries `requests-http` deliberately as a
name the typo rule should *not* fire on. Pretending one rule catches both would
be a lie this book's own ablation table would expose.

## What it deliberately ignores

Workspace members and `link: true` entries are first-party and skipped before
any clause runs. In `npm-xl` that is 14 of 1,390 entries. Without the exclusion
every monorepo scan is mostly noise about the project scanning itself.

## Checking a finding

You cannot do it from the lockfile — that is the whole point. Open the registry
page for the name. If it does not exist, you have your answer. If it exists but
was published last week by an account with no history, you have a different and
more interesting answer.

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.package-lock.json | jq -r '.findings[].package'
```
