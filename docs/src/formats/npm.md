# npm

`package-lock.json`, lockfileVersion 2 and 3.

```console
$ ./target/release/stranger scan fixtures/npm-xl.package-lock.json

  npm-xl.package-lock.json 1,376 packages   (150 direct · 1,226 transitive · 14 workspace)

  ⚠  INSTALL SCRIPTS        8     arbitrary code at install time

  ⚠  TRIVIAL                29    (2.1% of third-party)

  ⚠  VERSION DRIFT          76    same package at 2+ versions in one tree

  risk 62/100    392ms    third-party deps used to compute this: 0
```

All five rules can fire on an npm tree, though `pinning` never will: every entry
records a resolved version, so every entry is exactly pinned.

## What it refuses

**lockfileVersion 1**, by name:

```console
$ ./target/release/stranger scan /tmp/v1
stranger: /tmp/v1/package-lock.json: lockfileVersion 1 is not supported; stranger reads 2 and 3. Run `npm install` with npm 7 or newer to upgrade the file.
```

Version 1 kept the tree in a nested `dependencies` object and has no `packages`
map at all. Refusing it beats mis-reading it: a reader that looked for `packages`
and found nothing would report a clean project with zero dependencies, which is
the worst possible output for an auditing tool.

**A file with no `lockfileVersion` field** is refused as not looking like a
package-lock.json. **A file with a `lockfileVersion` but no `packages` map** is
refused too.

**A filename that is not one of the six it knows:**

```console
$ ./target/release/stranger scan /tmp/renametest/requirements-dev.txt
stranger: requirements-dev.txt: not a lockfile stranger knows. It reads: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock
```

`Cargo.lock` was the example here until the Cargo reader landed and it started
reading fine. The list in the message comes from the same constant discovery
uses, so the message could not go stale — only this page could, and did.

## Keys are install paths, not names

The awkward part of the format is that `packages` is keyed by where npm put the
thing on disk:

```json
"node_modules/@babel/core": { ... },
"node_modules/eslint/node_modules/semver": { ... }
```

The package name is the segment after the **last** `node_modules/`, with a scope
slash after it counting as part of the name. 184 of `npm-xl`'s 1,390 entries are
nested like that, so it is not a rare path.

Resolving one package's dependency to another package's entry means reproducing
npm's own lookup: try the nearest `node_modules` directory, then walk up. A
dependency `c` of the package at `node_modules/a/node_modules/b` is the first of
these that exists:

```text
node_modules/a/node_modules/b/node_modules/c
node_modules/a/node_modules/c
node_modules/c
```

Getting that wrong does not produce a parse error. It produces a graph with the
wrong edges, which silently corrupts in-degree, which is the clause
[the detection rule](../detection/rule.md) leans on hardest. `tests/npm.rs`
asserts the nested count and that every nested entry keeps its own identity,
including scoped ones.

Those same nested keys are what [version drift](../rules/drift.md) reads: they are
how the format spells duplication.

## Which fields are read

| field | used for |
|---|---|
| `lockfileVersion` | accept or refuse the file |
| `version` | printed beside the name; compared for equality by the drift rule |
| `dev`, `optional` | recorded on each package; no rule uses them yet |
| `link` | marks a symlink to a workspace member as first-party |
| `hasInstallScript` | the [install scripts](../rules/install-scripts.md) rule |
| `integrity` | presence recorded; never verified, see [Limits](../limits.md) |
| `dependencies`, `devDependencies`, `optionalDependencies`, `peerDependencies` | graph edges |

`peerDependencies` is in that list on purpose. A peer dep is a real maintainer
writing down a real name, which is exactly the evidence the detection rule wants,
and counting it can only make the rule quieter. `devDependencies` only ever
appears on the root entry.

The empty-string key is the root project. It is in the map and is not one of the
dependencies being counted, which is why `npm-xl` reports 1,390 entries from a
1,391-entry map. It is also why the root project's own `hasInstallScript` does not
count as a supply-chain signal: that one is your build.

## First-party entries

An entry is first-party when its key contains no `node_modules/` at all — that is
a workspace directory like `apps/desktop` — or when it carries `"link": true`,
which is the symlink npm leaves in `node_modules` pointing at one. `npm-xl` has 14
of them, 7 of which are links.

First-party packages are skipped by every rule, never counted as direct
dependencies of themselves, and reported separately in the header's `workspace`
field. [Monorepos](../cookbook/monorepos.md) covers why their outgoing edges are
treated as manifest declarations rather than as evidence.

## Discovery

`stranger scan <dir>` recurses, skipping `node_modules` and eleven other
directories, and matches any filename ending in one of the six names it knows.

Pointing at a file skips the walk, and the match there is the same suffix rule, so
a lockfile you have renamed still reads:

```console
$ ./target/release/stranger scan fixtures/npm-s.package-lock.json

  npm-s.package-lock.json  405 packages   (12 direct · 393 transitive)

  ⚠  INSTALL SCRIPTS        3     arbitrary code at install time

  ⚠  TRIVIAL                10    (2.5% of third-party)

  ⚠  VERSION DRIFT          30    same package at 2+ versions in one tree

  risk 56/100    65ms    third-party deps used to compute this: 0
```

## Fixture counts

Every entry count was measured with `jq '.packages | length - 1'` against the
file, not copied from notes. The notes said `npm-xl` held 1,391 entries. It holds
1,390.

| fixture | entries | third-party | direct | workspace |
|---|---|---|---|---|
| `npm-xs` | 37 | 37 | 1 | 0 |
| `npm-s` | 405 | 405 | 12 | 0 |
| `npm-m` | 582 | 576 | 20 | 6 |
| `npm-l` | 754 | 754 | 32 | 0 |
| `npm-xl` | 1,390 | 1,376 | 150 | 14 |
| `poisoned` | 757 | 757 | 35 | 0 |

`poisoned` is `npm-l` plus three planted names, all inserted as root dependencies
with no parent. It added exactly three entries, three roots, and no edges —
`tests/npm.rs` asserts all three, because nothing depends on a hallucination.

```console
$ ./target/release/stranger scan -v fixtures/npm-m.package-lock.json
```
