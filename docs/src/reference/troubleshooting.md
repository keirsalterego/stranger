# Troubleshooting

## "no lockfile in ." on a project that has one

Discovery recurses from the directory you named and matches filenames ending in
one of six known names. A file renamed at the *front* still reads; one renamed at
the back does not.

```console
$ ./target/release/stranger scan /tmp/project

  no lockfile in /tmp/project
  looked for: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock
```

The usual cause is a name like `requirements-dev.txt`, which ends in `.txt` and
not in `requirements.txt`. Nothing reads file contents to second-guess the name,
so pointing straight at it does not help either — that is exit 2, "not a lockfile
stranger knows". Copy or symlink it to a name in the list.

The other cause is a lockfile inside `node_modules`, `target`, `.venv` or one of
the nine other skipped directories, or deeper than six levels down.

Point at a file to skip the walk:

```console
$ ./target/release/stranger scan fixtures/npm-s.package-lock.json

  npm-s.package-lock.json  405 packages   (12 direct · 393 transitive)

  ⚠  INSTALL SCRIPTS        3     arbitrary code at install time

  ⚠  TRIVIAL                10    (2.5% of third-party)

  ⚠  VERSION DRIFT          30    same package at 2+ versions in one tree

  risk 56/100    65ms    third-party deps used to compute this: 0
```

Matching on a file path is by suffix, so `npm-s.package-lock.json` and
`old.package-lock.json` both read.

Exit code here is 0, not an error. A repository of mixed languages should not turn
red on the directories the tool has nothing to say about.

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
$ ./target/release/stranger scan /tmp/renametest/requirements-dev.txt
stranger: requirements-dev.txt: not a lockfile stranger knows. It reads: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock
```

Six formats in this build, and the error lists them from the same constant
discovery uses, so the message cannot drift out of date the way this page did.

You asserted the file was a lockfile and it does not match any name, so this is
exit 2 rather than the exit 0 a directory scan gives. Renaming it to
`requirements.txt` reads it — the name is the whole test, and nothing looks
inside to confirm.

## "no `packages` map"

The file parsed as JSON and had a `lockfileVersion` of 2 or more, but no
`packages` object. Either it is truncated or it is not a package-lock.

## A syntax error with a line and column

```console
$ echo '{"lockfileVersion":3, "packages" 1}' > /tmp/bad/package-lock.json
$ ./target/release/stranger scan /tmp/bad/package-lock.json
stranger: expected ':' at 1:34
```

pip errors quote the fragment as well as the position:

```console
$ printf 'flask[async>=3.0\n' > /tmp/bad/requirements.txt
$ ./target/release/stranger scan /tmp/bad/requirements.txt
stranger: `flask[async>=3.0` has an unclosed `[` in its extras at 1:1
```

Line and column are 1-based and the column counts characters, not bytes, so it
lines up with what an editor shows you. A lockfile that fails to parse is usually
a merge conflict marker or a truncated download.

## Nothing found on a lockfile you expect findings in

Work through the three clauses of the detection rule in order. It fires only when
all three hold.

1. **The name is in the corpus.** 140,066 npm names and 15,000 PyPI names are
   compiled in; if the name is one of them the rule stops immediately. This is the
   case for a typosquat that actually got registered.
2. **No corpus name is within edit distance 2.** A hallucinated name that is not a
   near-miss of a real one has no parent for clause 2 to find. Check the range is
   really empty before concluding this — `requests-http` was assumed to be in this
   category and turned out to be two edits from `requests-html`.
3. **Something depends on it.** If any third-party package in the tree lists the
   name, the rule goes quiet by design. Edges out of the root manifest and out of
   workspace members do not count for this; edges out of any other package do. On a
   `requirements.txt` there are no edges at all, so this clause never suppresses
   anything.

Also check the package is not first-party. Workspace directories and `link: true`
entries are skipped by every rule before any clause runs.

## Only the critical block is listed

By design. Non-critical rules collapse to a count and a reason, because a
1,390-package tree produces 76 drift findings and 29 trivial ones. `-v` expands
them:

```console
$ ./target/release/stranger scan -v fixtures/npm-xs.package-lock.json

  npm-xs.package-lock.json 37 packages   (1 direct · 36 transitive)

  ⚠  TRIVIAL                4     (10.8% of third-party)
     es-errors@1.3.0          one expression, one publisher · inlining it removes an account from your build
     gopd@1.2.0               one expression, one publisher · inlining it removes an account from your build
     has-symbols@1.1.0        predicate-shaped, resolves nothing · size not measured, see rule docs
     hasown@2.0.4             one expression, one publisher · inlining it removes an account from your build

  risk 9/100    13ms    third-party deps used to compute this: 0
```

`--format json` is never collapsed — it emits every finding whether or not you
passed `-v`.

## Findings you believe are wrong

Most likely the package is real and newer than the corpus snapshot of 2026-08-28,
or it fell off a popularity ranking. `tensorflow-gpu` in `fixtures/reqs-xs.
requirements.txt` is exactly that, and it is shipped as a fixture rather than
hidden. [False positives](../detection/false-positives.md) covers the shape of it
and the [ablation table](../detection/ablation.md) puts numbers on it.

The nearest name in `detail` is the closest corpus entry, which is not always the
name you would have guessed, and gets less reliable as the corpus ages. Treat it as
the rule showing its working.

For `TRIVIAL` hits specifically: that rule cannot see how long a file is, and
reports a good share of packages that are not one-liners.
[Trivial packages](../rules/trivial.md) says which and why.

## No colour

Colour is on when stdout is a TTY. Four inputs decide, highest priority first:
`--no-color`, then `NO_COLOR`, then `CLICOLOR_FORCE`, then TTY. Off beats on at
every tie.

If a CI log renders ANSI and you want colour there, set `CLICOLOR_FORCE=1`. If you
are getting escape codes where you do not want them, `--no-color` beats everything
else. `--format json` never carries colour under any setting.

## A closed pipe prints nothing and exits 0

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json | head -3

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

$ echo $?
0
```

EPIPE is the shell working correctly, not a failure, so it is silent. If a piped
run reports an error, the error is real and came from somewhere else.

## "scan takes one path"

```console
$ ./target/release/stranger scan a/package-lock.json b/package-lock.json
stranger: scan takes one path; got a second, `b/package-lock.json`
```

One path per run, on purpose — a second argument being silently ignored is worse.
Loop in the shell. A directory containing both a `package-lock.json` and a
`requirements.txt` is one path and scans both.

```console
$ ./target/release/stranger scan -v fixtures/poisoned.requirements.txt
```
