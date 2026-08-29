# stranger, and the site serving this page

Two scans that are worth running because of what they are, not what they find.

## The empty manifest, checked by the tool that argues for it

`stranger`'s whole claim is that it has no dependencies. It can make that claim
about itself, in its own output format, using the same reader it points at anybody
else's project:

```console
$ ./target/release/stranger scan Cargo.lock

  Cargo.lock               0 packages   (0 direct · 0 transitive · 1 workspace)

  no findings
  ·  INSTALL SCRIPTS        — no signal in this format
  ·  UNPINNED               — no signal in this format

  risk 0/100    0ms    third-party deps used to compute this: 0
```

**0 packages.** The one workspace entry is `stranger` itself — a `[[package]]`
with no `source` key, because there is nowhere to fetch it from, which is the same
test [the cargo reader](../formats/cargo.md) applies to any path dependency.

That last line of the footer is fixed text, not a computation. It says the same
thing on every scan, and this is the one run where you can check it against the
line above it.

The other two ways to ask are in
[Reproducible builds](../reference/reproducible-builds.md) and `deps-proof.txt`:

```console
$ cargo tree | wc -l
1

$ grep -c '^\[\[package\]\]' Cargo.lock
1
```

Three different questions — the resolver's, the lockfile's, and the auditor's —
and they agree.

## The site that serves this book

This page is published at `keir.is-a.dev/stranger/`. The domain is served by an
Astro site in a separate repository, and that repository has a
`pnpm-lock.yaml` with 850 packages in it. Running `stranger` against the thing
hosting `stranger`'s documentation is the closing check, and it is published
whatever it says:

```console
$ ./target/release/stranger scan ~/keir.is-a.dev

  pnpm-lock.yaml           850 packages   (29 direct · 821 transitive)

  ⚠  HALLUCINATION RISK     1
     taze@19.0.4              not in corpus · d=1 from "gaze" · root-only, no parent

  ⚠  TRIVIAL                23    (2.7% of third-party)

  ⚠  VERSION DRIFT          58    same package at 2+ versions in one tree

  ·  INSTALL SCRIPTS        — no signal in this format
  ·  UNPINNED               — no signal in this format

  risk 77/100    233ms    third-party deps used to compute this: 0
```

### The finding is wrong, and it is the useful kind of wrong

[`taze`](https://www.npmjs.com/package/taze) is a real, maintained npm package for
updating dependency ranges. It is a **false positive**, and the reason is exactly
the limit the corpus section names: 140,066 names is the most-downloaded slice of
npm, `taze` sits outside it, and the rule cannot tell "below the popularity cut"
from "does not exist". Clause 3 had no chance to save it either — nothing depends
on `taze`, because a devDependency of the root manifest genuinely has in-degree
zero. That is the rule working correctly on bad information.

The neighbour it names, `gaze`, is one deletion away and real.

So the honest reading of this scan is: one finding, and it is wrong. That is what
`stranger` says about the site that hosts it, and tuning it away would have been a
better screenshot and a worse tool.
[False positives](../detection/false-positives.md) has the other two.

### The lockfile is already a fixture

`fixtures/pnpm-l.pnpm-lock.yaml` is that file, byte for byte:

```console
$ sha256sum ~/keir.is-a.dev/pnpm-lock.yaml fixtures/pnpm-l.pnpm-lock.yaml
a04b16fb54b274f40d9fef0dbad27616c1e6755409383c5a07e106075c23981a  …/keir.is-a.dev/pnpm-lock.yaml
a04b16fb54b274f40d9fef0dbad27616c1e6755409383c5a07e106075c23981a  fixtures/pnpm-l.pnpm-lock.yaml
```

Which means the pnpm reader was developed against this exact file and `taze` has
been a known false positive since before the reader shipped. It was left in rather
than removed from the fixture, and it is named in the README's limits, because a
detector whose test data has had its inconvenient case deleted is measuring
nothing.

## Try it on yours

```console
$ ./target/release/stranger scan ~/some-project
```

No install, no resolve, no network, and no toolchain for the ecosystem you are
auditing — see [a project whose toolchain you do not have](no-toolchain.md).
