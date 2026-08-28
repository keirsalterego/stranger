# Troubleshooting

## "no lockfile in ." on a project that has one

Discovery looks for exactly `package-lock.json`, directly in the directory you
named, and does not recurse.

```console
$ ./target/release/stranger scan fixtures

  no lockfile in fixtures
  looked for: package-lock.json
```

The fixtures in this repository are all renamed, so the directory scans as empty.
Point at the file:

```console
$ ./target/release/stranger scan fixtures/npm-s.package-lock.json

  npm-s.package-lock.json  405 packages   (12 direct · 393 transitive)

  no findings
  risk 0/100    135ms    third-party deps used to compute this: 0
```

Matching on a file path is by suffix, so `npm-s.package-lock.json` and
`old.package-lock.json` both read.

Exit code here is 0, not an error. A repository of mixed languages should not
turn red on the directories the tool has nothing to say about.

## "lockfileVersion 1 is not supported"

```console
$ ./target/release/stranger scan /tmp/v1
stranger: /tmp/v1/package-lock.json: lockfileVersion 1 is not supported; stranger reads 2 and 3. Run `npm install` with npm 7 or newer to upgrade the file.
```

Do what it says. npm 7 and later write version 2 or 3, which have the `packages`
map this reader needs. Version 1 kept the tree in a nested `dependencies` object
and cannot be read the same way.

## "not a lockfile stranger knows"

```console
$ ./target/release/stranger scan fixtures/cargo-s.Cargo.lock
stranger: cargo-s.Cargo.lock: not a lockfile stranger knows. It reads: package-lock.json
```

One format in this build. The error lists what it reads, and that list comes
from the same constant discovery uses, so it cannot drift out of date.

## "no `packages` map"

The file parsed as JSON and had a `lockfileVersion` of 2 or more, but no
`packages` object. Either it is truncated or it is not a package-lock.

## A syntax error with a line and column

```console
$ echo '{"lockfileVersion":3, "packages" 1}' > /tmp/bad/package-lock.json
$ ./target/release/stranger scan /tmp/bad
stranger: expected ':' at 1:34
```

Line and column are 1-based and the column counts characters, not bytes, so it
lines up with what an editor shows you. Open the file at that position. A
lockfile that fails to parse is usually a merge conflict marker or a truncated
download.

## Nothing found on a lockfile you expect findings in

Work through the three clauses in order. The rule fires only when all three hold.

1. **The name is in the corpus.** 140,066 npm names are compiled in; if the name
   is one of them the rule stops immediately. This is the case for a typosquat
   that actually got registered.
2. **No corpus name is within edit distance 2.** A hallucinated name that is not
   a near-miss of a real one — `api-client-utils`, say — has no parent for
   clause 2 to find.
3. **Something depends on it.** If any third-party package in the tree lists the
   name, the rule goes quiet by design. Edges out of the root manifest and out of
   workspace members do not count for this; edges out of any other package do.

Also check the package is not first-party. Workspace directories and `link: true`
entries are skipped before any clause runs.

## Findings you believe are wrong

Most likely the package is real and newer than the corpus snapshot of
2026-08-28. [False positives](../detection/false-positives.md) covers the shape
of this and the [ablation table](../detection/ablation.md) puts numbers on it.

The nearest name in `detail` is the closest corpus entry, which is not always the
name you would have guessed. Treat it as the rule showing its work.

## "stdout: Broken pipe"

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json | true
stranger: stdout: Broken pipe (os error 32)
```

Whatever was on the right of the pipe closed it before the report finished
writing. The message goes to stderr and the exit code is 2. It is harmless, and
it usually means the downstream command failed for its own reasons — check that
one first.

## "scan takes one path"

```console
$ ./target/release/stranger scan a/package-lock.json b/package-lock.json
stranger: scan takes one path; got a second, `b/package-lock.json`
```

One path per run, on purpose — a second argument being silently ignored is worse.
Loop in the shell.

## `make bench` or `make proof` fails

```console
$ make bench
make: ./scripts/bench.sh: No such file or directory
make: *** [Makefile:10: bench] Error 127
```

Both targets are declared and the scripts are not in the tree yet.

## `--no-color` appears to do nothing

It does nothing. This build emits no colour, so there is none to suppress. The
flag is accepted so that scripts written against it keep working.

## `-q` still prints the header

`--quiet` currently suppresses only the "no lockfile" message. The report header
and footer are printed either way. Listed under [Limits](../limits.md) with the
other flags that do less than the help text implies.

```console
$ ./target/release/stranger scan fixtures/npm-xs.package-lock.json
```
