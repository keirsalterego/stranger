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

An empty file is the other one, and it used to be the worst answer this tool
could give:

```console
$ mkdir -p /tmp/empty-yarn
$ printf '' > /tmp/empty-yarn/yarn.lock
$ ./target/release/stranger scan /tmp/empty-yarn/yarn.lock
stranger: /tmp/empty-yarn/yarn.lock: no entries and no `# yarn lockfile v1` header; this is not the lockfile its name claims to be
```

Every other reader here refuses a file that is not the thing its name claims —
`Cargo.lock` wants a `[[package]]`, `go.mod` wants a `module`, `poetry.lock`
wants a `metadata.lock-version`. This one had no such check, because the v1
header is a comment and a hand-edited or concatenated lockfile can be missing it
while its entries are perfectly good. That reasoning holds for a file *with*
entries and quietly fails for a file without any: a truncated or zero-byte
`yarn.lock` read as a clean tree of nothing, `risk 0/100`, and a `--fail-on`
gate went green over a lockfile nobody had read.

So the header is required only when there is no other evidence. A project with
no dependencies really does produce a header and nothing else, and that is still
zero packages rather than an error.

## Nested blocks

An entry's fields are `key value` pairs, except for the ones that are a bare
`name:` opening a block indented under it. Six of them turn up:

| block | read as |
|---|---|
| `dependencies`, `optionalDependencies`, `peerDependencies` | graph edges |
| `engines`, `os`, `cpu`, `dependenciesMeta`, `peerDependenciesMeta` | consumed and dropped |

`peerDependencies` is an edge for [the same reason it is one in npm](npm.md): a
peer dep is a real maintainer writing down a real name, which is the evidence
[the detection rule](../detection/rule.md) wants, and an in-edge can only make
that rule quieter. Both readers produce `Ecosystem::Npm` and
[`stranger diff`](../using/diff.md) will put one against the other, so a name
with in-degree 1 under npm must not have in-degree 0 under yarn — otherwise
migrating a project between the two reports findings the migration did not
introduce.

`engines` is the counter-case and the reason the block matters rather than the
line: `node ">=6"` is the same two-token shape as a dependency line and names
no package at all.

The bare header itself is the trap. `peerDependencies:` has no value on its
line, so a field scanner that splits every line into `key value` has nothing to
split and reports a syntax error — against a file that has none, and against
most of the real yarn v1 lockfiles in existence, since peer dependencies are
ordinary.

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
