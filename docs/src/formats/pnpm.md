# pnpm

`pnpm-lock.yaml`, lockfileVersion 9.

```console
$ ./target/release/stranger scan fixtures/pnpm-l.pnpm-lock.yaml

  pnpm-l.pnpm-lock.yaml    850 packages   (29 direct · 821 transitive)

  ⚠  TRIVIAL                23    (2.7% of third-party)

  ⚠  VERSION DRIFT          58    same package at 2+ versions in one tree

  ·  INSTALL SCRIPTS        — no signal in this format
  ·  UNPINNED               — no signal in this format

  risk 46/100    148ms    third-party deps used to compute this: 0
```

`taze` used to appear in that block, and was the npm half of a pair of false
positives — `ksni` on [cargo](cargo.md) was the other. Both are four characters
long, and that is why: on npm, a four-character name has a neighbour within two
edits **100%** of the time, so clause 2 was a formality rather than a filter. The
length budget refuses it the edit and `taze` stops firing.
[False positives](../detection/false-positives.md) has the table.

It is still in the fixture. A fixture that stops demonstrating the bug it was
kept for is still the file the fix was measured against.

pnpm packages are npm packages, so this reader shares npm's corpus and the trivial
rule's name list works here unchanged. What it does not share is the file.

## Three sections, and they are not interchangeable

| section | keyed by | what it carries |
|---|---|---|
| `importers` | workspace directory, `.` for a single-package repo | the project's own manifests: `name: {specifier, version}` |
| `packages` | `name@version` | 850 distinct tarballs — `resolution`, `engines`, `hasBin`, `peerDependencies`, `deprecated` |
| `snapshots` | `name@version` **plus a peer suffix** | the installed instances. **This is where the edges are.** |

Reading edges out of `packages` instead of `snapshots` gets you a package list and
no graph, which is the failure mode that matters: the
[detection rule](../detection/rule.md)'s third clause needs in-degree, and a tree
with no edges makes every package look like a root.

## Peer suffixes

The same tarball can be installed more than once with different peers resolved, so
a snapshot key carries a parenthesised suffix its `packages` entry does not:

```text
packages:    astro@5.7.10
snapshots:   astro@5.7.10(@types/node@22.15.3)(jiti@2.4.2)
```

Dependency *values* carry it too — `'@volar/kit': 2.4.23(typescript@5.8.3)`. Both
ends of every edge are truncated at the first `(` before lookup.

Splitting these on `@` is how a naive reader loses every scoped package.
`@babel/core@7.27.1` has to split at the **last** `@`, and the `@types/node@22.15.3`
inside a suffix must not be split at all. `tests/pnpm.rs` asserts the 1,851 edges
resolve and that scoped names survive.

## What the format does not record

**Install scripts.** lockfileVersion 6 had `requiresBuild`. Version 9 dropped it,
and this fixture has nothing equivalent. `hasBin: true` appears 42 times and is
**not** a substitute — it means the package ships a `bin` to symlink, not that code
runs at install time. Mapping one to the other would put 42 High findings in the
report, every one invented. `install_script` is false for every package here, so
[install scripts](../rules/install-scripts.md) says nothing about a pnpm tree, and
that silence means *not measured*.

**Dev-only packages.** pnpm 9 records dev-ness on the importer's manifest, not on
the package, and does not mark the transitive closure. `dev` is false throughout
rather than guessed at with a graph walk.

## There are no first-party packages, by construction

A workspace member lives in `importers` and never in `packages`, and pnpm writes
its dependents' references to it as `link:../name`, which resolves to no package
entry. So `first_party` is false everywhere and the header prints no `workspace`
count — where the [npm reader](npm.md) has to work for exactly the same
distinction. Different file, same idea, much less code.

## What it refuses

```console
$ mkdir -p /tmp/v6
$ printf "lockfileVersion: '6.0'\n\nimporters:\n  .: {}\n" > /tmp/v6/pnpm-lock.yaml
$ ./target/release/stranger scan /tmp/v6/pnpm-lock.yaml
stranger: /tmp/v6/pnpm-lock.yaml: lockfileVersion 6.0 is not supported; stranger reads 9. Run `pnpm install` with pnpm 9 or newer to upgrade the file.
```

Version 6 and below have no `snapshots` section at all, so this reader would find
no edges and report a tree in which nothing depends on anything. Refusing by name
beats that.

The version arrives as the string `"9.0"`, because pnpm quotes it and
[the YAML subset](../decisions.md) types exactly two tokens — `true` and `false` —
and leaves everything else a string. `no` and `on` are npm package names this
repository's own corpus can confirm — `y` and `off` are registered too but sit
below its popularity cut —
and a reader that turned `no@1.0.0` into a boolean would drop a package out of an
audit without saying anything.

```console
$ ./target/release/stranger scan -v fixtures/pnpm-l.pnpm-lock.yaml
```
