# A project whose toolchain you do not have

You have a repository in a language you do not build. No `node`, no `npm`, no
`cargo`, no `python`. You still want to know what its dependency tree contains.

```console
$ ./target/release/stranger scan .
```

The lockfile is already on disk. Somebody's toolchain wrote it, and it records
resolved versions, integrity fields, install-script flags and the dependency edges
between them. Reading it needs a JSON parser, not a package manager.

## Why this is allowed

The hackathon rules forbid shelling out to installed tooling. Their FAQ rules this
design in explicitly:

> Parsing files those tools already produced is fine, because nothing
> third-party ends up in your artifact.

Two conditions attach and both are met.

**Disclosed.** The corpus of known-real names and the fixture lockfiles are
written up in
[corpus/PROVENANCE.md](https://github.com/keirsalterego/stranger/blob/main/corpus/PROVENANCE.md),
[fixtures/README.md](https://github.com/keirsalterego/stranger/blob/main/fixtures/README.md)
and `STDLIB.md`, with dates, sources and counts. Both are data. Nothing there is
compiled as source, nothing executes, and the corpus is embedded with
`include_str!` and read as text.

**Degrades gracefully.** No lockfile is not an error. The tool says what it looked
for and exits 0:

```console
$ ./target/release/stranger scan /tmp/empty

  no lockfile in /tmp/empty
  looked for: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock

$ echo $?
0
```

That message names the formats rather than saying "nothing found", so the answer
to "why did it not scan my project" is in the output instead of in the source. The
list comes from the same constant discovery uses, so it cannot drift out of date.

## The corpus degrades honestly too

If the compiled-in name list were empty, every name would fail clause 1, the
nearest-neighbour search would find nothing, and the rule would report nothing. An
absent corpus makes the tool quiet, not hysterical.

That is not hypothetical. `Ecosystem::Go` runs in exactly that state on purpose:
`proxy.golang.org` publishes no ranked list and module paths are domains, so there
is no Go corpus and the detection rule can never fire on one. Named in
[Limits](../limits.md) rather than shipped as a rule that silently does nothing.

## A filename it does not recognise

```console
$ ./target/release/stranger scan /tmp/renametest/requirements-dev.txt
stranger: requirements-dev.txt: not a lockfile stranger knows. It reads: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock
$ echo $?
2
```

The match is on the end of the filename and nothing reads the contents to
second-guess it, so a file renamed at the front still works and one renamed at the
back does not. It is a name it does not know rather than a format it cannot parse —
that file is a perfectly ordinary `requirements.txt` under another name.

Pointing at a specific file you believe is a lockfile and being wrong is a usage
error — exit 2. Scanning a directory that happens to hold nothing readable is not
— exit 0. The difference is whether you asserted something.

All six formats read, which is the point of this page: `Cargo.lock` needs no
`cargo`, `uv.lock` needs no `uv`, and `pnpm-lock.yaml` needs no `pnpm` or Node.
A semver comparator (`src/semver.rs`) exists and is tested and is not wired into
the binary, because no rule has yet asked an ordering question.

## No network, checkable

```console
$ cargo tree
stranger v0.1.0 (/home/keir/stranger)

$ grep -c '^\[\[package\]\]' Cargo.lock
1
```

One package. Rust's standard library has no TLS and no HTTP client, so a binary
with no dependencies cannot open a socket. CI enforces all three of those — the
one-line `cargo tree`, no `unsafe` in `src/`, and no `Command::new` anywhere — so
the claim fails the build rather than quietly rotting.

Run the scan with the cable out and it behaves identically. So does the build:

```console
$ CARGO_NET_OFFLINE=true cargo build --release --locked --offline
    Finished `release` profile [optimized] target(s) in 0.01s
```

```console
$ ./target/release/stranger scan /tmp/empty; echo $?
```
