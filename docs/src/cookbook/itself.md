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

  ⚠  TRIVIAL                23    (2.7% of third-party)

  ⚠  VERSION DRIFT          58    same package at 2+ versions in one tree

  ·  INSTALL SCRIPTS        — no signal in this format
  ·  UNPINNED               — no signal in this format

  risk 46/100    146ms    third-party deps used to compute this: 0
```

### There used to be a hallucination finding here, and it was wrong

This section said, for most of the weekend, that the scan above reported one
`HALLUCINATION RISK` and that it was a false positive. It is gone from the block —
not tuned away, and the difference matters.

[`taze`](https://www.npmjs.com/package/taze) is a real, maintained npm package for
updating dependency ranges, it sits outside the most-downloaded 140,066 names, and
clause 3 had no chance to save it: nothing depends on `taze`, because a
devDependency of the root manifest genuinely has in-degree zero. The neighbour the
tool named, `gaze`, is one deletion away and real. The page called that the rule
working correctly on bad information, and left it in on the grounds that a better
screenshot would have made a worse tool.

That reading was too generous to the rule. `taze` is **four characters long**, and
a four-character name has a neighbour within two edits 100% of the time on npm —
so clause 2 was not weighing evidence about `taze`, it was passing everything. The
finding was not the corpus being incomplete. It was the threshold being a constant
where it should have been a function of length.
[`distance::CHARS_PER_EDIT`](../detection/false-positives.md) is the fix and the
measurement behind it.

So what this page now shows is a scan with no hallucination finding on it, which
is a weaker screenshot and a truer one. The rule still gets `tensorflow-gpu`
wrong, in a `requirements.txt` fixture, for a reason no length policy can fix —
that one is fourteen characters, and at fourteen characters a near-miss really is
evidence. [False positives](../detection/false-positives.md) keeps it.

### The lockfile is already a fixture

`fixtures/pnpm-l.pnpm-lock.yaml` is that file, byte for byte:

```console
$ sha256sum ~/keir.is-a.dev/pnpm-lock.yaml fixtures/pnpm-l.pnpm-lock.yaml
a04b16fb54b274f40d9fef0dbad27616c1e6755409383c5a07e106075c23981a  …/keir.is-a.dev/pnpm-lock.yaml
a04b16fb54b274f40d9fef0dbad27616c1e6755409383c5a07e106075c23981a  fixtures/pnpm-l.pnpm-lock.yaml
```

Which means the pnpm reader was developed against this exact file and `taze` was a
known false positive from before the reader shipped until the last day of the
window. It was left in rather than removed from the fixture, and that is the only
reason the length budget could be measured against it at all: a detector whose
test data has had its inconvenient case deleted has nothing left to measure a fix
with.

## Try it on yours

```console
$ ./target/release/stranger scan ~/some-project
```

No install, no resolve, no network, and no toolchain for the ecosystem you are
auditing — see [a project whose toolchain you do not have](no-toolchain.md).
