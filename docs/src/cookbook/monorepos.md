# Monorepos

A workspace member's `package.json` is a manifest, not evidence.

That sentence is the whole page, and it changes both numbers a monorepo scan
produces.

## The counting problem

npm records the whole workspace in one `package-lock.json` at the root. In a
workspace layout the root manifest usually declares almost nothing — the actual
dependencies live in `apps/*/package.json` and `packages/*/package.json`, and
the lockfile carries them as entries keyed by directory.

A reader that took direct dependencies from the root entry alone would report
this:

```text
  npm-m.package-lock.json  582 packages   (0 direct · 582 transitive)
```

Zero direct dependencies for a 582-package project. Both monorepo fixtures here
have that shape. What the reader actually reports:

```console
$ ./target/release/stranger scan fixtures/npm-m.package-lock.json

  npm-m.package-lock.json  582 packages   (20 direct · 562 transitive)

  no findings
  risk 0/100    29ms    third-party deps used to compute this: 0
```

20 direct, because the dependencies a workspace member declares are dependencies
this repository chose. Check the root entry yourself and the gap is stark:

```console
$ jq '.packages[""] | {deps: (.dependencies|length), workspaces}' fixtures/npm-m.package-lock.json
{
  "deps": 0,
  "workspaces": [
    "apps/*",
    "packages/*"
  ]
}
```

`npm-xl` is the same story at larger scale. Its root entry declares 2
dependencies across 5 workspace globs; the reader reports 150 direct out of
1,390.

## The detection problem

The counting is cosmetic. The consequence for the rule is not.

The detection rule fires only when nothing depends on a name. If an edge out of
`apps/desktop` counted as a normal dependency edge, then a hallucinated name
added to `apps/desktop/package.json` would arrive with in-degree 1, clause 3
would suppress it, and the tool would never look at it again. Exactly the
scenario the tool exists for, hidden by exactly the layout most large JavaScript
projects use.

So edges out of a first-party entry go into `roots`, not `edges`. Same manifest,
same author, same absence of evidence as the root `package.json`.

```console
$ mkdir -p /tmp/c && cat > /tmp/c/package-lock.json <<'EOF'
{
  "name": "monorepo",
  "lockfileVersion": 3,
  "packages": {
    "": { "workspaces": ["apps/*"] },
    "apps/desktop": { "dependencies": { "expres": "^4.18.2" } },
    "node_modules/desktop": { "resolved": "apps/desktop", "link": true },
    "node_modules/expres": { "version": "4.18.2" }
  }
}
EOF
$ ./target/release/stranger scan /tmp/c

  package-lock.json        3 packages   (1 direct · 2 transitive)

  ⚠  HALLUCINATION RISK     1
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent

  risk 25/100    19ms    third-party deps used to compute this: 0
```

The workspace member declared the name and it is still reported. Put the same
name behind a third-party package and it is not — that pair is on
[The co-occurrence rule](../detection/rule.md).

## Workspace members are not findings

Two kinds of entry are first-party. A key with no `node_modules/` in it is a
workspace directory. A `"link": true` entry is the symlink npm leaves in
`node_modules` pointing at one.

Both are skipped before any clause runs, and neither is ever reported as a
direct dependency of itself. `npm-xl` has 14 of them, 7 links among them.
Without that exclusion, a monorepo scan is mostly the project complaining about
its own package names.

One rough edge: `transitive` is `packages - direct`, so those 14 first-party
entries are counted as transitive in the header. They are neither, really. It is
a two-number summary of a graph and something has to give.

## One lockfile, one scan

Discovery is not recursive, so `stranger scan .` at a workspace root finds the
one lockfile npm actually wrote and stops. That is the right file — the
per-member `package.json` files are not lockfiles and have no resolved versions
in them.

```console
$ ./target/release/stranger scan fixtures/npm-xl.package-lock.json
```
