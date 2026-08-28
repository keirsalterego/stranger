# Version drift

One name, installed at more than one version, in one tree.

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.package-lock.json | jq -r '.findings[] | select(.rule=="drift") | "\(.package)  \(.detail)"' | head -8
@types/unist  2 versions: 2.0.11, 3.0.3
ajv  2 versions: 6.12.6, 8.20.0
ajv-formats  2 versions: 2.1.1, 3.0.1
ansi-regex  2 versions: 5.0.1, 6.2.2
balanced-match  2 versions: 1.0.2, 4.0.4
brace-expansion  3 versions: 1.1.12, 2.0.2, 5.0.7
chalk  2 versions: 4.1.2, 5.6.2
commander  2 versions: 11.1.0, 14.0.3
```

## Why the lockfile already knows

npm deduplicates what it can and nests what it cannot. When two packages want
incompatible ranges of the same name, the loser gets its own copy at
`node_modules/parent/node_modules/name`.

Those nested keys are not a quirk of the file format — they *are* how the format
spells duplication. 184 of `npm-xl`'s 1,390 entries are nested. So this rule needs
no resolver, no registry and no `node_modules` on disk. It is reading the answer
npm already wrote down.

## Why medium

Nothing is exploitable because `ansi-regex` is installed at both 5.0.1 and 6.2.2.
The argument is about the next advisory rather than today.

When a CVE lands on that name, the bump you make in your own manifest moves the
copy your manifest reaches and leaves the other one pinned by whoever nested it.
The fix reads as done while the vulnerable code is still on disk. Duplication is
the thing that turns patching into a negotiation.

Not high, because there is no vulnerability here yet. Not low, because it decides
how much tomorrow costs.

## One finding per name

`npm-xl` has 76 drifted names behind 180 distinct versions:

```console
$ ./target/release/stranger scan --format json fixtures/npm-xl.package-lock.json | jq '[.findings[] | select(.rule=="drift") | (.detail | capture("(?<n>[0-9]+) versions") | .n | tonumber)] | {names: length, versions: add}'
{
  "names": 76,
  "versions": 180
}
```

Reporting each of those 180 would be a wall; reporting the 76 names is something
you read. So the finding carries the name, an empty `version`, and the full
version list in `detail`:

```json
{"rule":"drift","severity":"medium","package":"brace-expansion","version":"","detail":"3 versions: 1.1.16, 2.1.2, 5.0.7"}
```

An empty `version` field in the JSON is how a consumer tells this rule's findings
apart from the others.

## Versions are compared for equality, never for order

The list is sorted in byte order, not semver order, which shows:

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.package-lock.json | jq -r '.findings[] | select(.rule=="drift" and .package=="minimatch") | .detail'
3 versions: 10.2.5, 3.1.2, 9.0.5
```

`10.2.5` sorts first because `1` sorts before `3`. The rule never asks which of
two versions is newer, only whether they differ, so a semver comparator would buy
nothing here except a wrong answer on the first `1.0.0-rc.1` it met.

`src/semver.rs` does exist and does implement precedence correctly, including the
prerelease rules from section 11 that most implementations get wrong by accident.
No rule calls it yet. When one needs "how far apart are these two versions", it
is there.

## What it cannot see

Whether the two copies matter. Two versions of a type-definitions package is
noise; two versions of a crypto library is not, and the lockfile records nothing
that would tell them apart.

It also cannot see duplication that npm resolved away. If your tree happens to
have deduplicated to one version today, a range in some transitive manifest can
still float it apart tomorrow, and nothing in this file predicts that.

## Not on pip

`requirements.txt` is a flat list of names, so a name appearing twice is a
conflict pip would reject rather than drift it would nest. The rule technically
runs on pip trees and cannot fire on a well-formed one.

```console
$ ./target/release/stranger scan -v fixtures/npm-l.package-lock.json | tail -20
```
