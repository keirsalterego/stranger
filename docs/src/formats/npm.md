# npm

`package-lock.json`, lockfileVersion 2 and 3. That is the whole list of formats
this build reads.

```console
$ ./target/release/stranger scan fixtures/npm-xl.package-lock.json

  npm-xl.package-lock.json 1,390 packages   (150 direct · 1,240 transitive)

  no findings
  risk 0/100    396ms    third-party deps used to compute this: 0
```

## What it refuses

**lockfileVersion 1**, by name:

```console
$ ./target/release/stranger scan /tmp/v1
stranger: /tmp/v1/package-lock.json: lockfileVersion 1 is not supported; stranger reads 2 and 3. Run `npm install` with npm 7 or newer to upgrade the file.
```

Version 1 kept the tree in a nested `dependencies` object and has no `packages`
map at all. Refusing it beats mis-reading it: a reader that looked for
`packages` and found nothing would report a clean project with zero
dependencies, which is the worst possible output for an auditing tool.

**A file with no `lockfileVersion` field** is refused as not looking like a
package-lock.json. **A file with a `lockfileVersion` but no `packages` map** is
refused too.

**Any other lockfile**, by filename:

```console
$ ./target/release/stranger scan fixtures/cargo-s.Cargo.lock
stranger: cargo-s.Cargo.lock: not a lockfile stranger knows. It reads: package-lock.json
```

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
wrong edges, which silently corrupts in-degree, which is the clause the
detection rule leans on hardest. `tests/npm.rs` asserts the nested count and
that every nested entry keeps its own identity, including scoped ones.

## Which fields are read

| field | used for |
|---|---|
| `lockfileVersion` | accept or refuse the file |
| `version` | printed beside the name |
| `dev`, `optional` | recorded on each package |
| `link` | marks a symlink to a workspace member as first-party |
| `hasInstallScript` | recorded; no rule consumes it yet |
| `integrity` | presence recorded; never verified, see [Limits](../limits.md) |
| `dependencies`, `devDependencies`, `optionalDependencies`, `peerDependencies` | graph edges |

`peerDependencies` is in that list on purpose. A peer dep is a real maintainer
writing down a real name, which is exactly the evidence the rule wants, and
counting it can only make the rule quieter. `devDependencies` only ever appears
on the root entry.

The empty-string key is the root project. It is in the map and is not one of the
dependencies being counted, which is why `npm-xl` reports 1,390 packages from a
1,391-entry map. It is also why the root project's own `hasInstallScript` does
not count as a supply-chain signal: that one is your build.

## First-party entries

An entry is first-party when its key contains no `node_modules/` at all — that
is a workspace directory like `apps/desktop` — or when it carries `"link": true`,
which is the symlink npm leaves in `node_modules` pointing at one. `npm-xl` has
14 of them, 7 of which are links.

First-party packages are skipped by the detection rule and never counted as
direct dependencies of themselves. [Monorepos](../cookbook/monorepos.md) covers
why their outgoing edges are treated as manifest declarations rather than as
evidence.

## Discovery

`stranger scan <dir>` looks for exactly `package-lock.json`, directly in that
directory. It does not recurse — a walk that wanders into `node_modules` and
audits four hundred vendored lockfiles is worse than no walk at all.

Pointing at a file skips discovery entirely, and the match there is on suffix,
so a lockfile you have renamed still reads:

```console
$ ./target/release/stranger scan fixtures/npm-s.package-lock.json

  npm-s.package-lock.json  405 packages   (12 direct · 393 transitive)

  no findings
  risk 0/100    135ms    third-party deps used to compute this: 0
```

## Fixture counts

Every number here was measured with `jq '.packages | length - 1'` against the
file, not copied from notes. The notes said `npm-xl` held 1,391 entries. It
holds 1,390.

| fixture | packages | direct | notes |
|---|---|---|---|
| `npm-xs` | 37 | 1 | |
| `npm-s` | 405 | 12 | |
| `npm-m` | 582 | 20 | 3 `link: true` workspace members |
| `npm-l` | 754 | 32 | |
| `npm-xl` | 1,390 | 150 | 14 first-party, 184 nested, 8 install scripts |
| `poisoned` | 757 | 35 | `npm-l` plus three planted names |

```console
$ ./target/release/stranger scan fixtures/npm-l.package-lock.json
```
