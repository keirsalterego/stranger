# Exit codes

Three of them, and the split between 1 and 2 is the point.

| code | meaning |
|---|---|
| 0 | clean, or findings below the `--fail-on` threshold, or no lockfile found |
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

```console
$ ./target/release/stranger scan --fail-on critical fixtures/poisoned.package-lock.json > /dev/null
$ echo $?
1

$ ./target/release/stranger scan --fail-on high fixtures/npm-xl.package-lock.json > /dev/null
$ echo $?
0
```

The threshold is compared against the worst severity seen. Levels order
`low < medium < high < critical`, so `--fail-on low` fails on anything at all
and `--fail-on critical` fails only on the top level.

Today every finding the tool can produce is `critical`, so all four thresholds
behave identically. That will stop being true when a second rule lands. Pick the
level you mean now rather than the level that happens to work.

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

$ ./target/release/stranger scan fixtures/cargo-s.Cargo.lock
stranger: cargo-s.Cargo.lock: not a lockfile stranger knows. It reads: package-lock.json
$ echo $?
2
```

Errors go to stderr, findings to stdout. Because the reader parses a lockfile
before anything is written, a `--format json` run that exits 2 has printed
nothing to stdout — a downstream parser gets an empty stream and a non-zero
status rather than half an object.

## No lockfile is not an error

```console
$ ./target/release/stranger scan /tmp/empty

  no lockfile in /tmp/empty
  looked for: package-lock.json

$ echo $?
0
```

Exit 0. Running the tool across a repository of mixed languages should not turn
red on the directories it has nothing to say about. See
[A project whose toolchain you do not have](../cookbook/no-toolchain.md).

```console
$ ./target/release/stranger scan --fail-on high fixtures/poisoned.package-lock.json; echo $?
```
