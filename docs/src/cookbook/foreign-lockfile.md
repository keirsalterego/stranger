# Audit a lockfile you did not write

Somebody sends you a repository. A contractor's handover, a candidate's
take-home, a dependency you are about to vendor, an npm package you are
considering. You want to know what is in the tree before you type `npm install`,
because `npm install` is the part that runs other people's code.

```console
$ ./target/release/stranger scan path/to/their/package-lock.json
```

Nothing is installed, nothing is resolved, no `node_modules` appears, and the
registry is never contacted. The lockfile is read as text.

## Why the order matters

The usual way to inspect a dependency tree is to install it first and then ask the
installed tree questions. That is backwards for an unknown repository: the install
is the risky operation.

That risk is measurable from the file itself, which is the point of the
[install scripts](../rules/install-scripts.md) rule:

```console
$ ./target/release/stranger scan -v fixtures/npm-m.package-lock.json | head -8

  npm-m.package-lock.json  576 packages   (20 direct · 556 transitive · 6 workspace)

  ⚠  INSTALL SCRIPTS        4     arbitrary code at install time
     esbuild@0.27.7                         runs code at install time · lockfile records the flag, not the script
     fsevents@2.3.3                         runs code at install time · lockfile records the flag, not the script
     sharp@0.34.5                           runs code at install time · lockfile records the flag, not the script
     unrs-resolver@1.12.2                   runs code at install time · lockfile records the flag, not the script
```

Four packages whose code runs before yours does, named before you install any of
them. Reading the lockfile inverts the order: the tool never executes anything
from the tree, and it has nothing of its own to execute either — an empty
dependency manifest means auditing a hostile lockfile cannot pull a hostile
parser.

## On a file, not a directory

Point at the whole repository and discovery does the work: it walks six levels
down, skipping `node_modules` and twelve other named directories, and picks up
any of the six filenames in `lock::KNOWN` — `package-lock.json`,
`pnpm-lock.yaml`, `Cargo.lock`, `requirements.txt`, `poetry.lock`, `uv.lock`.
On a handover that is what you want, because you do not yet know which
ecosystems are in there.

Point at a single file and discovery is skipped entirely. The format is then
chosen by the same suffix match, so an archived or renamed lockfile still reads:

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

  risk 81/100    141ms    third-party deps used to compute this: 0
```

One path per run. A second positional argument is a usage error rather than a
silently ignored one:

```console
$ ./target/release/stranger scan a/package-lock.json b/package-lock.json
stranger: scan takes one path; got a second, `b/package-lock.json`
```

Loop in the shell if you have several:

```console
$ for f in */package-lock.json; do ./target/release/stranger scan "$f"; done
```

## Reading the result

The header tells you the size of what you were about to install. `757 packages (35
direct · 722 transitive)` means 35 names somebody chose and 722 that came along.

Read the critical block first — it is listed in full because it is the answer.
Then decide whether the counts underneath deserve a `-v`. On a tree you did not
write, `INSTALL SCRIPTS` usually does.

A finding tells you a name has no evidence behind it, or that a package runs code,
or that something is installed twice. None of that says the package is malicious,
and a clean scan does not say the tree is safe. [Limits](../limits.md) is the
honest list of what is not checked, and integrity hashes are at the top of it.

## What it will not tell you

Whether the `sha512-…` integrity fields are correct. Whether an install script is
benign. Whether a package that genuinely exists is nonetheless hostile. Those need
either cryptography or the network, and this binary has neither.

```console
$ ./target/release/stranger scan -v fixtures/npm-xl.package-lock.json | head -14
```
