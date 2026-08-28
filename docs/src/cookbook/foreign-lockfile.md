# Audit a lockfile you did not write

Somebody sends you a repository. A contractor's handover, a candidate's take-home,
a dependency you are about to vendor, an npm package you are considering. You
want to know what is in the tree before you type `npm install`, because
`npm install` is the part that runs other people's code.

```console
$ ./target/release/stranger scan path/to/their/package-lock.json
```

Nothing is installed, nothing is resolved, no `node_modules` appears, and the
registry is never contacted. The lockfile is read as text.

## Why the order matters

The usual way to inspect a dependency tree is to install it first and then ask
the installed tree questions. That is backwards for an unknown repository: the
install is the risky operation. `hasInstallScript` on a package-lock entry means
arbitrary code runs during that install, and you find out which packages carry
it by installing them.

Reading the lockfile inverts that. The tool never executes anything from the
tree, and it has nothing of its own to execute either — an empty dependency
manifest means auditing a hostile lockfile cannot pull a hostile parser.

## On a file, not a directory

Directory discovery looks for exactly `package-lock.json` in exactly that
directory. Pointing at a file skips discovery, and the format match there is on
suffix, so an archived or renamed lockfile still reads:

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

  ⚠  HALLUCINATION RISK     3
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent

  risk 75/100    54ms    third-party deps used to compute this: 0
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

The header tells you the size of what you were about to install. `757 packages
(35 direct · 722 transitive)` means 35 names somebody chose and 722 that came
along.

A finding tells you a name has no evidence behind it. It does not tell you the
package is malicious, and a clean scan does not tell you the tree is safe — one
rule is implemented, and it only looks for names that resemble a real name.
[Limits](../limits.md) is the honest list of what this does not check, and
integrity hashes are at the top of it.

## What it will not tell you

Whether the `sha512-…` integrity fields are correct. Whether an install script
is benign. Whether a package that genuinely exists is nonetheless hostile.
Those need either cryptography or the network, and this binary has neither.

```console
$ ./target/release/stranger scan fixtures/npm-xl.package-lock.json
```
