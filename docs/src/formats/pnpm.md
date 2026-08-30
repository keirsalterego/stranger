# pnpm

`pnpm-lock.yaml`, lockfileVersion 9 and 6.

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

## `deprecated` is a block scalar

The one field in that table that is not a flow collection or a scalar on its
own line. pnpm copies the registry's deprecation message verbatim, and those
run to paragraphs, so it writes them as a YAML literal block:

```yaml
  q@1.5.1:
    resolution: {integrity: sha512-kV/CThkXo6xyFEZUugw…}
    engines: {node: '>=0.6.0', teleport: '>=0.2.0'}
    deprecated: |-
      You or someone you depend on is using Q, the JavaScript Promise library
      that gave JavaScript developers strong feelings about promises.

      (For a CapTP with native promises, see @endo/eventual-send)
```

Nothing here reads the message. What matters is that the reader gets to the end
of it and finds `engines:` again rather than treating an indented English
sentence as structure — so [the YAML subset](../decisions.md) parses literal
block scalars with all three chomping modes (`|`, `|-`, `|+`) rather than
refusing them. Refusing meant refusing the whole lockfile, and any tree holding
one deprecated package holds one of these.

The folded form `>` is still refused by name, along with an explicit
indentation indicator (`|2`). Neither appears in a lockfile, and a folded
scalar that joins its lines slightly wrong still parses — which is the failure
this subset is arranged against.

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

Everything in this section is about **version 9**. Version 6 records all three
and is read for all three, which is the odd part: the newer file is the one that
tells you less.

**Install scripts.** Version 9 dropped `requiresBuild` and did not replace it.
`hasBin: true` appears 42 times in this fixture and is **not** a substitute — it
means the package ships a `bin` to symlink, not that code runs at install time.
Mapping one to the other would put 42 High findings in the report, every one
invented. So `install_script` is false for every package in a v9 file and
[install scripts](../rules/install-scripts.md) says nothing about that tree, and
that silence means *not measured* rather than *nothing found*.

**Dev-only packages.** pnpm 9 records dev-ness on the importer's manifest, not on
the package, and does not mark the transitive closure. `dev` is false throughout
rather than guessed at with a graph walk.

**First-party packages.** A v9 workspace member lives in `importers` and never in
`packages`, and pnpm writes its dependents' references to it as `link:../name`,
which resolves to no package entry. So `first_party` is false everywhere in a v9
file for want of anything to be true about, and the header prints no `workspace`
count — where the [npm reader](npm.md) has to work for the same distinction.

## Version 6 is the same file with two sections fused

pnpm 8 wrote it, and it is still on disk in a great many repositories — one of
the four `pnpm-lock.yaml` files on the machine this was written on. The fixture
is mongodb/mongo's own lockfile, taken as found:

```console
$ ./target/release/stranger scan fixtures/pnpm-v6.pnpm-lock.yaml

  pnpm-v6.pnpm-lock.yaml   89 packages   (7 direct · 82 transitive · 1 workspace)

  ⚠  TRIVIAL                2     (2.2% of third-party)

  ⚠  VERSION DRIFT          2     same package at 2+ versions in one tree

  ·  UNPINNED               — no signal in this format

  risk 29/100    <ms>    third-party deps used to compute this: 0
```

Four differences, and the reader is otherwise the v9 reader:

- **No `snapshots` section.** A `packages` entry carries its own resolved
  `dependencies`, so the tarball list and the instance list are one section.
- **Keys start with `/`**, and the peer suffix rides on the `packages` key
  rather than on a separate snapshot key. Two entries for one tarball at
  different peers collapse to the single `Package` v9 would have written, which
  is what keeps a package count comparable across the two versions.
- **A single project writes its manifest at the top level** and no `importers`
  at all, so the document itself is the one importer.
- **It records `requiresBuild`, `dev`, and local packages**, all three of which
  v9 gave up.

That last one has teeth. A `file:` dependency gets a real `packages` entry keyed
by its path, with its actual name in a `name` field because the key has no
version to carry one:

```yaml
  file:buildscripts/eslint-plugin-mongodb:
    resolution: {directory: buildscripts/eslint-plugin-mongodb, type: directory}
    name: eslint-plugin-mongodb
    dev: false
```

Read literally, that audits the project's own code as a stranger under the name
`file:buildscripts/eslint-plugin-mongodb`, and `stranger tree` could not find it
under the name its own repository uses. Taking the `name` field and setting
`first_party` fixes both, and is the same call the [npm reader](npm.md) makes
about a workspace key — which is why the header above says `1 workspace`:

```console
$ ./target/release/stranger tree eslint-plugin-mongodb fixtures/pnpm-v6.pnpm-lock.yaml

  fixtures/pnpm-v6.pnpm-lock.yaml   npm · 89 packages

  eslint-plugin-mongodb   file:buildscripts/eslint-plugin-mongodb
     workspace member — your own code, not a stranger
```

## What it refuses

```console
$ mkdir -p /tmp/v5
$ printf "lockfileVersion: '5.4'\n" > /tmp/v5/pnpm-lock.yaml
$ ./target/release/stranger scan /tmp/v5/pnpm-lock.yaml
stranger: /tmp/v5/pnpm-lock.yaml: lockfileVersion 5.4 is not supported; stranger reads 9 and 6. Run `pnpm install` with pnpm 8 or newer to upgrade the file.
```

Version 5 and below key their packages `/name/version` rather than
`/name@version`. The last `@` in a scoped key is then part of the name and there
is no version to find, so a reader that guessed would split every scoped name in
the wrong place. Refusing by name beats that.

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
