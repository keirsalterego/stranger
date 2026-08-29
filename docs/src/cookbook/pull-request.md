# Gate a pull request

The lockfile is the artefact worth gating. It is the file a model's suggestion
ends up in, it is the file nobody reads in a diff of 4,000 lines, and it is the
file that decides what actually gets installed.

```console
$ ./target/release/stranger scan --fail-on high .
```

Exit 1 if anything at or above `high` is found, 0 otherwise, 2 if the run itself
was broken.

## GitHub Actions

```yaml
name: supply chain
on: [pull_request]

jobs:
  stranger:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/checkout@v4
        with:
          repository: keirsalterego/stranger
          path: .stranger
      - run: make -C .stranger
      - run: ./.stranger/target/release/stranger scan --fail-on high .
```

No toolchain setup step, no cache step, no registry credentials. The build has
nothing to fetch, so the job works on a runner with the network firewalled off.

## Pick a threshold and mean it

The five rules occupy four severities, so the threshold decides what the gate is
actually for.

| threshold | catches |
|---|---|
| `critical` | hallucinated names only |
| `high` | plus install scripts, plus unconstrained pip requirements |
| `medium` | plus version drift, plus pip ranges |
| `low` | plus trivial packages |

`--fail-on high` is the reasonable default: hallucinated names and code that runs
at install time, without failing a build over `is-number`.

`npm-xl` demonstrates the difference — it has install scripts and drift but no
hallucinated names:

```console
$ ./target/release/stranger scan --fail-on critical fixtures/npm-xl.package-lock.json > /dev/null
$ echo $?
0

$ ./target/release/stranger scan --fail-on high fixtures/npm-xl.package-lock.json > /dev/null
$ echo $?
1
```

Note that `--fail-on low` will fail almost every real npm tree, because almost
every real npm tree contains a micro-package. That is a statement about npm, not a
bug, but it makes `low` unusable as a gate.

## What the gate prints

```console
$ ./target/release/stranger scan --fail-on high fixtures/poisoned.package-lock.json

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

  ⚠  HALLUCINATION RISK     3
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent

  ⚠  INSTALL SCRIPTS        3     arbitrary code at install time

  ⚠  TRIVIAL                35    (4.6% of third-party)

  ⚠  VERSION DRIFT          55    same package at 2+ versions in one tree

  risk 100/100    141ms    third-party deps used to compute this: 0
$ echo $?
1
```

The critical block is listed in full and the rest collapses to counts, which is
the right shape for a CI log. Add `-v` if you want the whole thing in the job
output; add `-q` to drop the header and risk line.

Colour is off automatically, because a CI log is not a TTY. If your runner
renders ANSI and you want it, set `CLICOLOR_FORCE=1`.

## Failing the build is not the only option

A finding is "no evidence this name is real", not proof of an attack. If your tree
carries new packages often enough that a hard gate would cry wolf, post the report
instead of blocking:

```console
$ ./target/release/stranger scan --format json . > stranger.json
```

Exit code 0, machine-readable output, everything listed regardless of `-v`, and a
human decides. The [JSON output](../using/json.md) page has the field list, and
[False positives](../detection/false-positives.md) is worth reading before you
choose which mode you want.

## Do not gate on the risk score

`risk` is severity weights summed and capped at 100, and it saturates on any real
tree — every fixture here with more than one rule firing scores 100. It is not
calibrated against anything. Comparing today's score with yesterday's on the same
project is meaningful; comparing two projects is not, and a threshold on it would
be a number pretending to be a measurement. `--fail-on` compares severities, which
are at least defined.

```console
$ ./target/release/stranger scan --fail-on high fixtures/poisoned.package-lock.json; echo $?
```
