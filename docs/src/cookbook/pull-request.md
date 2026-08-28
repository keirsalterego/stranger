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

## What the gate sees

On a clean tree:

```console
$ ./target/release/stranger scan --fail-on high fixtures/npm-xl.package-lock.json

  npm-xl.package-lock.json 1,390 packages   (150 direct · 1,240 transitive)

  no findings
  risk 0/100    396ms    third-party deps used to compute this: 0
$ echo $?
0
```

On a tree somebody pasted a name into:

```console
$ ./target/release/stranger scan --fail-on high fixtures/poisoned.package-lock.json

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

  ⚠  HALLUCINATION RISK     3
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent

  risk 75/100    54ms    third-party deps used to compute this: 0
$ echo $?
1
```

## Picking a threshold

Every finding this build can produce is `critical`, so `--fail-on low` and
`--fail-on critical` behave identically today. Write the one you mean —
`--fail-on high` is the reasonable default — so that the gate does not change
behaviour underneath you when a lower-severity rule lands.

## Failing the build is not the only option

A finding is "no evidence this name is real", not proof of an attack. If your
tree carries new packages often enough that a hard gate would cry wolf, post the
report instead of blocking:

```console
$ ./target/release/stranger scan --format json . > stranger.json
```

Exit code 0, machine-readable output, and a human decides. The
[JSON output](../using/json.md) page has the field list, and
[False positives](../detection/false-positives.md) is worth reading before you
choose which mode you want.

## Do not gate on the risk score

`risk` is severity weights summed and capped at 100. It is not calibrated
against anything. Comparing today's score with yesterday's on the same project
is meaningful; comparing two projects is not, and a threshold on it would be a
number pretending to be a measurement. `--fail-on` compares severities, which
are at least defined.

```console
$ ./target/release/stranger scan --fail-on high fixtures/poisoned.package-lock.json; echo $?
```
