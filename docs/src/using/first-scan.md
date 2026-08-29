# Your first scan

`fixtures/poisoned.package-lock.json` is a real 754-package lockfile with three
fake names added by hand. Scan it:

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

  ⚠  HALLUCINATION RISK     3
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent

  ⚠  INSTALL SCRIPTS        3     arbitrary code at install time

  ⚠  TRIVIAL                35    (4.6% of third-party)

  ⚠  VERSION DRIFT          55    same package at 2+ versions in one tree

  ·  UNPINNED               — no signal in this format

  risk 81/100    71ms    third-party deps used to compute this: 0
```

## The header

```text
  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)
```

The lead number counts third-party packages only. *Direct* is the count named by
a manifest in this repository — the root `package.json`, and any workspace
member's. *Transitive* is everything reached only through another package. A
workspace member is neither, so it gets a third field of its own when there is
one:

```console
$ ./target/release/stranger scan fixtures/npm-xl.package-lock.json

  npm-xl.package-lock.json 1,376 packages   (150 direct · 1,226 transitive · 14 workspace)

  ⚠  INSTALL SCRIPTS        8     arbitrary code at install time

  ⚠  TRIVIAL                29    (2.1% of third-party)

  ⚠  VERSION DRIFT          76    same package at 2+ versions in one tree

  ·  UNPINNED               — no signal in this format

  risk 62/100    382ms    third-party deps used to compute this: 0
```

The file holds 1,390 entries. 14 of them are your own code, so the tree you got
from strangers is 1,376. [Monorepos](../cookbook/monorepos.md) covers why that
split is the same one the detection rule runs on.

## Collapsed rules

Critical findings are always listed. Everything else reports a count and what the
count means, because a 1,390-package tree produces 76 drift findings and 29
trivial ones, and printing all of them buries the three that matter under a
hundred lines nobody scrolls back through.

`-v` prints the lot:

```console
$ ./target/release/stranger scan -v fixtures/npm-xs.package-lock.json

  npm-xs.package-lock.json 37 packages   (1 direct · 36 transitive)

  ⚠  TRIVIAL                4     (10.8% of third-party)
     es-errors@1.3.0          one expression, one publisher · inlining it removes an account from your build
     gopd@1.2.0               one expression, one publisher · inlining it removes an account from your build
     has-symbols@1.1.0        predicate-shaped, resolves nothing · size not measured, see rule docs
     hasown@2.0.4             one expression, one publisher · inlining it removes an account from your build

  ·  UNPINNED               — no signal in this format

  risk 9/100    6ms    third-party deps used to compute this: 0
```

`-q` drops the header and the risk line and prints findings only, which is the
form to pipe into something:

```console
$ ./target/release/stranger scan -q fixtures/poisoned.package-lock.json

  ⚠  HALLUCINATION RISK     3
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent

  ⚠  INSTALL SCRIPTS        3     arbitrary code at install time

  ⚠  TRIVIAL                35    (4.6% of third-party)

  ⚠  VERSION DRIFT          55    same package at 2+ versions in one tree
```

## A finding

```text
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent
```

The three fields after the version are the three clauses of the rule, in the
terms the rule actually used, so you can disagree with it:

- **not in corpus** — `lodahs` is not one of the 140,066 npm names compiled into
  the binary.
- **d=1 from "lodash"** — its Damerau-Levenshtein distance to `lodash` is 1. One
  transposition. Under plain Levenshtein it would be 2, which is why the distance
  function is the one it is.
- **root-only, no parent** — nothing else in these 757 packages depends on it.

All three have to hold. [The co-occurrence rule](../detection/rule.md) is why, and
[`stranger tree lodahs`](tree.md) prints the in-edges the third one is about, so
you do not have to take that line's word for it.

Every rule writes its own `detail` in its own terms. The other four are
[install scripts](../rules/install-scripts.md),
[version drift](../rules/drift.md), [trivial packages](../rules/trivial.md) and
[unpinned requirements](../rules/pinning.md).

## Colour

Findings are coloured by the worst severity in their block: red for critical,
orange for high, yellow for medium, dim for low. Sixteen-colour SGR only, so it
renders the way your theme intends rather than the way the author's monitor
looked.

Four inputs decide whether escapes are emitted, highest priority first:

1. `--no-color` — you said so, out loud, this run.
2. `NO_COLOR` — off regardless of TTY.
3. `CLICOLOR_FORCE` — on regardless of TTY.
4. stdout is a TTY.

Off beats on at every tie, so a stray `CLICOLOR_FORCE` in a CI image cannot spray
escapes into a log that asked for none. Both variables count only when present
*and* non-empty, so `NO_COLOR=` means nothing was said rather than "off".

A pipe is not a TTY, so piped output is plain bytes you can grep:

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json | cat -v | head -4

  poisoned.package-lock.json 757 packages   (35 direct M-BM-7 722 transitive)

  M-bM-^ZM-   HALLUCINATION RISK     3
```

The `M-BM-7` is the UTF-8 middle dot, not an escape code. `--format json` never
carries colour under any setting: a program that has to strip SGR codes out of a
string field will not.

## The footer

```text
  risk 81/100    56ms    third-party deps used to compute this: 0
```

The risk number is a band for the worst severity present, plus position inside the
band for how many findings share it:

| worst finding | band |
|---|---|
| critical | 75–98 |
| high | 50–73 |
| medium | 25–48 |
| low | 1–24 |
| nothing | 0 |

The band is the same question `--fail-on` asks, on purpose — the headline number
and the gate should not disagree about what is serious. Volume saturates inside
the band and never fills it, so a worse severity always outranks more of a lesser
one, and there is always a worse tree than the one in front of you.

It is not calibrated against anything, because there is nothing honest to
calibrate it against. Two projects are comparable at the band; two scans of the
same project are comparable outright. The findings are the output; the score is a
handle.

This was a sum of severity weights capped at 100 until the cap turned out to be
doing all the work — nine of the sixteen fixtures scored exactly 100, including
both `poisoned.package-lock.json` and the clean `npm-l` it was built from.

The milliseconds are wall time for that run, measured by the tool. It moves
around. `make bench` runs the largest fixture fifty times:

```text
Benchmark 1: target/release/stranger scan fixtures/npm-xl.package-lock.json
  Time (mean ± σ):     413.0 ms ±  82.5 ms    [User: 404.8 ms, System: 3.4 ms]
  Range (min … max):   371.2 ms … 660.2 ms    50 runs
```

Most of that is the nearest-neighbour scan for names that miss the corpus.

```console
$ ./target/release/stranger scan -v fixtures/npm-m.package-lock.json
```
