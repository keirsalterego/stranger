# yarn

`yarn.lock`, the v1 format.

```console
$ ./target/release/stranger scan fixtures/yarn-l.yarn.lock

  yarn-l.yarn.lock         593 packages   (26 direct · 567 transitive)

  ⚠  TRIVIAL                23    (3.9% of third-party)

  ⚠  VERSION DRIFT          82    same package at 2+ versions in one tree

  ·  INSTALL SCRIPTS        — no signal in this format
  ·  UNPINNED               — no signal in this format

  risk 46/100    <ms>    third-party deps used to compute this: 0
```

The fixtures are three real lockfiles: npm packages ship their own inside their
tarballs, so `pdf-lib` supplies a 4,408-line one and `uri-js` a 2,558-line one.
Neither was written for this reader.

## Not YAML, despite reading like it

yarn v1 wrote its own format: entries at column 0, two-space fields under each,
and quoting that is optional except where a value would otherwise break the
line. `yaml.rs` is next door and deliberately unused — a YAML parser hands back
`lodash@^4.17.20, lodash@~4.17.0` as one scalar key that then has to be taken
apart anyway.

## The edges are specifiers, not versions

This is the one thing that makes yarn different from every other format here,
and getting it wrong produces a tree with no edges rather than an error.

A `dependencies` line names a **range**:

```text
"@babel/code-frame@^7.0.0":
  version "7.0.0"
  dependencies:
    "@babel/highlight" "^7.0.0"
```

The entry `"@babel/highlight" "^7.0.0"` points at is the one whose key list
contains the literal specifier `@babel/highlight@^7.0.0`. Only the key list can
answer that: `^7.0.0` never appears in the target's `version` field, which says
`7.0.0`. A reader that matched on the resolved version would find nothing, parse
every entry successfully, and report a package count with an empty edge set —
which reads as "nothing depends on anything" and hands every package an
in-degree of zero, the exact shape [the co-occurrence
rule](../detection/rule.md) fires on.

One entry answers to several keys:

```text
"@babel/generator@^7.9.0", "@babel/generator@^7.9.5":
  version "7.9.5"
```

75 of the 593 entries in `yarn-l` are written that way, and the file depends on
that one through both keys. A reader that took only the first would silently
lose every edge arriving through the second.

## What it refuses

```console
$ mkdir -p /tmp/berry
$ printf '__metadata:\n  version: 8\n' > /tmp/berry/yarn.lock
$ ./target/release/stranger scan /tmp/berry/yarn.lock
stranger: /tmp/berry/yarn.lock: this is a Yarn Berry (v2+) lockfile; stranger reads the v1 format
```

Berry is a real YAML document keyed `name@npm:range` with its own `__metadata`
version counter. Same filename, different format.

## What the format does not record

**The root manifest.** Direct dependencies live in `package.json`, which is not
this file, so `roots` is derived the way [poetry's](poetry-uv.md) is: an entry
nothing else depends on can only have arrived through the root. The derivation
is wrong in one direction — a direct dependency something else also needs has an
in-edge and drops out of the count.

**Install scripts, dev-ness, and workspace membership.** None of the three is in
a v1 lockfile, so [install scripts](../rules/install-scripts.md) reports *no
signal in this format* rather than *nothing found*.

**Integrity.** Present on all 593 entries of `yarn-l` and on none of `yarn-xs`,
whose yarn predates the field. Reported as present or absent, never as checked —
[std has no crypto](../limits.md).
