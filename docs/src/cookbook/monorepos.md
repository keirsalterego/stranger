# Monorepos

A workspace member's `package.json` is a manifest, not evidence.

That sentence is the whole page, and it changes both numbers a monorepo scan
produces.

## The counting problem

npm records the whole workspace in one `package-lock.json` at the root. In a
workspace layout the root manifest usually declares almost nothing — the actual
dependencies live in `apps/*/package.json` and `packages/*/package.json`, and the
lockfile carries them as entries keyed by directory.

A reader that took direct dependencies from the root entry alone would report zero
direct dependencies for a 576-package project. Check the root entry yourself and
the gap is stark:

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

What the reader actually reports:

```console
$ ./target/release/stranger scan fixtures/npm-m.package-lock.json

  npm-m.package-lock.json  576 packages   (20 direct · 556 transitive · 6 workspace)

  ⚠  INSTALL SCRIPTS        4     arbitrary code at install time

  ⚠  TRIVIAL                17    (3.0% of third-party)

  ⚠  VERSION DRIFT          20    same package at 2+ versions in one tree

  ·  UNPINNED               — no signal in this format

  risk 58/100    12ms    third-party deps used to compute this: 0
```

20 direct, because the dependencies a workspace member declares are dependencies
this repository chose. `npm-xl` is the same story at larger scale: its root entry
declares 2 dependencies across 5 workspace globs, and the reader reports 150
direct out of 1,376.

## Three numbers, not two

`576 packages (20 direct · 556 transitive · 6 workspace)`.

The lead count is third-party only. A workspace member is neither a direct
dependency nor a transitive one — it is your own code — so it is excluded from
both and reported separately. The file holds 582 entries; 6 of them are yours.

The `workspace` field only appears when there is one. `npm-l` has none and prints
two numbers.

## The detection problem

The counting is cosmetic. The consequence for the rules is not.

The detection rule fires only when nothing depends on a name. If an edge out of
`apps/desktop` counted as a normal dependency edge, then a hallucinated name added
to `apps/desktop/package.json` would arrive with in-degree 1, clause 3 would
suppress it, and the tool would never look at it again. Exactly the scenario the
tool exists for, hidden by exactly the layout most large JavaScript projects use.

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
    "node_modules/expres": {
      "version": "4.18.2",
      "resolved": "https://registry.npmjs.org/expres/-/expres-4.18.2.tgz"
    }
  }
}
EOF
$ ./target/release/stranger scan /tmp/c

  package-lock.json        1 packages   (1 direct · 0 transitive · 2 workspace)

  ⚠  HALLUCINATION RISK     1
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent

  ·  UNPINNED               — no signal in this format

  risk 77/100    34ms    third-party deps used to compute this: 0
```

The workspace member declared the name and it is still reported. Put the same name
behind a third-party package and it is not — that pair is on
[The co-occurrence rule](../detection/rule.md).

## Workspace members are never findings

Two kinds of entry are first-party. A key with no `node_modules/` in it is a
workspace directory. A `"link": true` entry is the symlink npm leaves in
`node_modules` pointing at one.

Both are skipped before any rule runs, not only the detection rule. Your own
build scripts are not `INSTALL SCRIPTS` findings, and your own package names are
not `TRIVIAL` ones. `npm-xl` has 14 first-party entries, 7 links among them, and
`tests/rules.rs` asserts none of them ever appears in a finding.

## Discovery walks down, and the skip list is what makes that safe

`stranger scan .` at a workspace root does not read only that directory. It walks
down from there, six levels deep, sorting as it goes and never following a
symlink, and it refuses to enter `node_modules`, `target`, `dist`, `vendor`,
nine other names and every dot-directory.

On a plain npm workspace the walk still ends up with the one lockfile npm
actually wrote at the root, because the per-member `package.json` files are not
lockfiles and have no resolved versions in them. The recursion earns its keep on
the layouts where that is not true — a lockfile per app, or more than one
ecosystem:

```console
$ rm -rf /tmp/mono
$ mkdir -p /tmp/mono/apps/web /tmp/mono/services/api /tmp/mono/node_modules/vendored
$ cp fixtures/npm-xs.package-lock.json /tmp/mono/apps/web/package-lock.json
$ cp fixtures/npm-xs.package-lock.json /tmp/mono/node_modules/vendored/package-lock.json
$ cp fixtures/reqs-xs.requirements.txt /tmp/mono/services/api/requirements.txt
$ find /tmp/mono -type f | sort
/tmp/mono/apps/web/package-lock.json
/tmp/mono/node_modules/vendored/package-lock.json
/tmp/mono/services/api/requirements.txt
$ ./target/release/stranger scan --format json /tmp/mono | jq -r .source
/tmp/mono/apps/web/package-lock.json
/tmp/mono/services/api/requirements.txt
```

Three lockfiles on disk, two audited. The one under `node_modules` is somebody
else's vendored copy, and a populated `node_modules` holds hundreds of them —
walking into it turns one scan into four hundred irrelevant ones.
`tests/cli.rs::a_directory_scan_skips_vendored_lockfiles` asserts the skip, and
`a_directory_scan_is_deterministic` asserts the walk finds `Cargo.lock`,
`uv.lock` and `requirements.txt` across the fixtures directory in sorted order.

Each file found produces its own report block:

```console
$ rm -rf /tmp/mixed && mkdir -p /tmp/mixed
$ cp fixtures/poisoned.requirements.txt /tmp/mixed/requirements.txt
$ cat > /tmp/mixed/package-lock.json <<'EOF'
{
  "name": "mixed",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "mixed", "dependencies": { "expres": "4.18.2" } },
    "node_modules/expres": {
      "version": "4.18.2",
      "resolved": "https://registry.npmjs.org/expres/-/expres-4.18.2.tgz",
      "integrity": "sha512-AA"
    }
  }
}
EOF
$ ./target/release/stranger scan /tmp/mixed

  package-lock.json        1 packages   (1 direct · 0 transitive)

  ⚠  HALLUCINATION RISK     1
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent

  ·  UNPINNED               — no signal in this format

  risk 77/100    38ms    third-party deps used to compute this: 0


  requirements.txt         6 packages   (6 direct · 0 transitive)

  ⚠  HALLUCINATION RISK     2
     python-dateutils@2.9.0   not in corpus · d=1 from "python-dateutil" · root-only, no parent
     requests-http@1.0.2      not in corpus · d=2 from "requests-html" · root-only, no parent

  ⚠  UNPINNED               3     no exact version recorded

  ·  INSTALL SCRIPTS        — no signal in this format

  risk 79/100    34ms    third-party deps used to compute this: 0
```

`--fail-on` compares against the worst severity across all of them. `--format
json` emits one object per line rather than an array — see
[JSON output](../using/json.md).

```console
$ ./target/release/stranger scan -v fixtures/npm-xl.package-lock.json | head -3
```
