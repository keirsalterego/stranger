# Exit codes

Three of them, and the split between 1 and 2 is the point.

| code | meaning |
|---|---|
| 0 | clean, findings below the `--fail-on` threshold, or no lockfile found |
| 1 | a finding at or above the threshold |
| 2 | bad usage, or a file that could not be read or parsed |

A CI gate that cannot tell a finding from a broken invocation is a CI gate
somebody turns off. So a typo in a flag exits 2, a missing file exits 2, a
lockfile in a format the reader refuses exits 2, and only an actual finding
exits 1.

## Without `--fail-on`

Nothing ever exits 1. The scan reports and returns 0.

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json > /dev/null
$ echo $?
0
```

That is deliberate. `stranger scan` on its own is a thing you read; making it
fail by default would mean everyone learns to write `|| true`.

## With `--fail-on`

The threshold is compared against the worst severity seen. Levels order
`low < medium < high < critical`, and the five rules occupy four different
levels, so the choice matters.

| rule | severity |
|---|---|
| slopsquat | critical |
| install-script | high |
| pinning | high, medium or low, depending on the specifier |
| drift | medium |
| trivial | low |

`npm-xl` has install scripts, drift and trivial findings but no hallucinated
names, which makes it a clean demonstration of the threshold actually doing
something:

```console
$ ./target/release/stranger scan --fail-on critical fixtures/npm-xl.package-lock.json > /dev/null
$ echo $?
0

$ ./target/release/stranger scan --fail-on high fixtures/npm-xl.package-lock.json > /dev/null
$ echo $?
1
```

`npm-xs` has nothing but four trivial findings, so it separates `low` from
`medium`:

```console
$ ./target/release/stranger scan --fail-on low fixtures/npm-xs.package-lock.json > /dev/null
$ echo $?
1

$ ./target/release/stranger scan --fail-on medium fixtures/npm-xs.package-lock.json > /dev/null
$ echo $?
0
```

## Exit 2

```console
$ ./target/release/stranger scan fixtures/nope.json
stranger: fixtures/nope.json: no such file or directory
$ echo $?
2

$ ./target/release/stranger scan --format yaml fixtures/poisoned.package-lock.json
stranger: --format takes `human` or `json`, not `yaml`
$ echo $?
2

$ mkdir -p /tmp/renametest
$ printf 'flask==3.0.0\n' > /tmp/renametest/requirements-dev.txt
$ ./target/release/stranger scan /tmp/renametest/requirements-dev.txt
stranger: requirements-dev.txt: not a lockfile stranger knows. It reads: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock, go.mod, yarn.lock
$ echo $?
2
```

Errors go to stderr, findings to stdout. Because the reader parses a lockfile
before anything is written, a `--format json` run that exits 2 has printed
nothing to stdout — a downstream parser gets an empty stream and a non-zero
status rather than half an object.

## A closed pipe is not an error

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json | head -3

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

$ echo $?
0
```

`head` closes the pipe as soon as it has what it wants, and every write after
that fails with EPIPE. That is the shell working correctly, so it exits 0 and
says nothing. The alternative is an error message on every piped invocation.

## A directory it could not open is an error

```console
$ rm -rf /tmp/shut && mkdir -p /tmp/shut/proj
$ cp fixtures/poisoned.package-lock.json /tmp/shut/proj/
$ chmod 000 /tmp/shut/proj
$ ./target/release/stranger scan /tmp/shut --fail-on critical

  could not look inside 1 path — this scan is incomplete
     /tmp/shut/proj

$ echo $?
2
$ chmod 755 /tmp/shut/proj
```

Exit 2, and it **outranks the findings** — a directory that will not open is not
a bump, it is stranger being unable to do its job. `--fail-on` asks "is there a
finding at or above this level", and over a list of lockfiles that is short by an
unknown number the honest answer is neither 0 nor 1.

**The cost is real and it is worth naming.** One `0700` directory anywhere under
the scan root turns the gate red, whether or not it had a lockfile in it. That is
the trade: the alternative is the bug this replaced, where `chmod 000` over a
directory holding a poisoned lockfile printed `no lockfile` and exited 0. A green
tick over a directory nobody could open is worse than a red one over a directory
that turned out to be empty, because only one of those two failures is silent.

A directory that is *absent* is not a blind spot and is not counted — a path that
is not there hides nothing.

## No lockfile is not an error

```console
$ rm -rf /tmp/empty && mkdir -p /tmp/empty
$ ./target/release/stranger scan /tmp/empty

  no lockfile stranger reads in /tmp/empty
  looked for: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock, go.mod, yarn.lock

$ echo $?
0
```

Exit 0. Running the tool across a repository of mixed languages should not turn
red on the directories it has nothing to say about. See
[A project whose toolchain you do not have](../cookbook/no-toolchain.md).

```console
$ ./target/release/stranger scan --fail-on high fixtures/npm-xl.package-lock.json; echo $?
```
