# Comparing two lockfiles

`scan` answers *is this tree bad*. A reviewer looking at a pull request has a
narrower question, and it is not the same one: *did this change make it worse*.

```console
$ ./target/release/stranger diff fixtures/npm-l.package-lock.json fixtures/poisoned.package-lock.json

  fixtures/npm-l.package-lock.json -> fixtures/poisoned.package-lock.json

  added       3
     chalck@5.3.0
     expres@4.18.2
     lodahs@4.17.21

  introduced  4 findings
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent
     lodahs@4.17.21           runs code at install time · lockfile records the flag, not the script
```

`poisoned.package-lock.json` is `npm-l` with three names planted in it. A `scan`
of it also reports 55 version-drift findings and 35 trivial packages, every one
of which was already in `npm-l`. The diff is the three that arrived.

## Why the gate is not just two scans

`--fail-on` means something different here, deliberately. On `scan` it is the
worst finding in the tree; on `diff` it is the worst finding the change
*introduced*.

```console
$ ./target/release/stranger diff fixtures/npm-l.package-lock.json fixtures/poisoned.package-lock.json --fail-on high -q
$ echo $?
1
```

Reverse the arguments and the same two files pass, because taking a problem out
is not putting one in:

```console
$ ./target/release/stranger diff fixtures/poisoned.package-lock.json fixtures/npm-l.package-lock.json --fail-on high -q
$ echo $?
0
```

While `scan` fails on that tree in both directions:

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json --fail-on high -q
$ echo $?
1
```

That asymmetry is the reason the subcommand exists. A repository with 211
trivial packages fails a `scan` gate on every pull request until somebody turns
the gate off; it passes a `diff` gate until a pull request adds something.

## Findings are matched by rule and package, not by version

A bumped dependency keeps its findings. `esbuild@0.20.0` with an install script
becoming `esbuild@0.21.0` with an install script is not one finding fixed and
one introduced — it is a version change, which the `changed` block prints, and
no change in risk.

The cost is real: a package already flagged for a rule can change version and
the new version's finding is not called new.

## A finding can move without a package moving

The two lockfiles do not have to be the same format — only the same ecosystem —
and the formats do not all record the same things. npm records install scripts;
[yarn v1](../formats/yarn.md) has no field for them. So migrating a project
between the two changes the finding set while `added`, `removed` and `changed`
are all empty, and that is a real result rather than a glitch: the packages did
not move, but what is now *visible* about them did.

`diff` prints those findings and gates on them. The alternative — deciding
there is nothing to say because no package moved — is how it printed
`no change to the dependency tree` and exited 1 at the same time, which in CI
is a red build with nothing on screen to explain it.

## In CI

```console
$ git show HEAD~1:package-lock.json > /tmp/before.package-lock.json
$ stranger diff /tmp/before.package-lock.json package-lock.json --fail-on high
```

Exit 1 means this change introduced something at or above the threshold. See
[exit codes](exit-codes.md), and [gate a pull request](../cookbook/pull-request.md)
for the whole-tree version of the same job.

## JSON

One object, not a stream — a diff is one comparison however many packages it
touched. No timing field, for the same reason [the scan object has
none](json.md): two runs over the same pair have to produce the same bytes.

```console
$ ./target/release/stranger diff --format json fixtures/npm-l.package-lock.json fixtures/poisoned.package-lock.json | head -c 120
{"old":"fixtures/npm-l.package-lock.json","new":"fixtures/poisoned.package-lock.json","added":["chalck@5.3.0","expres@4.
```

## What it refuses

Two different ecosystems. A `Cargo.lock` against a `package-lock.json` parses
fine and produces a diff in which every package was added and every package was
removed — a confident, detailed, meaningless answer.

```console
$ ./target/release/stranger diff fixtures/cargo-s.Cargo.lock fixtures/npm-xs.package-lock.json
stranger: fixtures/cargo-s.Cargo.lock and fixtures/npm-xs.package-lock.json are different ecosystems (Crates and Npm); there is nothing to compare
```

Two *formats* of one ecosystem are fine, and that diff is the one somebody
migrating from npm to pnpm wants.
