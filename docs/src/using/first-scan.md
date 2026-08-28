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

  risk 75/100    54ms    third-party deps used to compute this: 0
```

## The header

```text
  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)
```

*Direct* is the count of packages named by a manifest in this repository — the
root `package.json`, and any workspace member's. *Transitive* is everything
else: `packages.len() - direct`. The distinction is not cosmetic. It is the
same split the detection rule runs on, and [Monorepos](../cookbook/monorepos.md)
covers why a workspace member counts as "this repository" rather than as a
dependency.

## A finding

```text
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent
```

The three fields after the version are the three clauses of the rule, in the
terms the rule actually used, so you can disagree with it:

- **not in corpus** — `lodahs` is not one of the 140,066 npm names compiled into
  the binary.
- **d=1 from "lodash"** — its Damerau-Levenshtein distance to `lodash` is 1. One
  transposition. Under plain Levenshtein it would be 2, which is why the
  distance function is the one it is.
- **root-only, no parent** — nothing else in these 757 packages depends on it.

All three have to hold. [The co-occurrence rule](../detection/rule.md) is why.

## The footer

```text
  risk 75/100    54ms    third-party deps used to compute this: 0
```

The risk number is severity weights summed and capped at 100: critical 25, high
10, medium 3, low 1. Three criticals is 75. It is not calibrated against
anything and there is nothing honest to calibrate it against — it exists so that
two scans of the same project can be compared. The findings are the output; the
score is a handle.

The milliseconds are wall time for that run, measured by the tool. It moves
around: the same file scanned three times in a row on this machine reported
53 ms, 53 ms and 54 ms, but the 1,390-package `npm-xl` fixture consistently
takes about 380 ms because it has more names that miss the corpus and each miss
costs a linear scan.

## A clean file

```console
$ ./target/release/stranger scan fixtures/npm-xl.package-lock.json

  npm-xl.package-lock.json 1,390 packages   (150 direct · 1,240 transitive)

  no findings
  risk 0/100    396ms    third-party deps used to compute this: 0
```

Five clean npm fixtures, 3,168 real packages, zero findings between them.

```console
$ ./target/release/stranger scan fixtures/npm-m.package-lock.json
```
