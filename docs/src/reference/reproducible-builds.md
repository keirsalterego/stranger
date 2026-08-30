# Reproducible builds

Same commit, same toolchain, two different directories, one binary.

```console
$ make repro
commit:  81716ac9e2ffa8178af1780378fe1591186d9870
rustc:   rustc 1.98.0 (88d9e12ae 2026-08-18)
epoch:   1787940000

build A  /tmp/stranger-repro.102959/a
         a7cb6a024249a28bd48884023406518e5fe3773a00bf8bb00f40cca84a2614de
build B  /tmp/stranger-repro.102959/b-with-a-deliberately-longer-name
         a7cb6a024249a28bd48884023406518e5fe3773a00bf8bb00f40cca84a2614de

MATCH — byte-identical across two directories
```

The hash is for whatever you have checked out, and it moves whenever `src/` or
`Cargo.lock` moves. **The line carrying the claim is `MATCH`, not the hash.**

Which is not a hypothetical caveat: this page and the README quoted two different
runs, at two different commits, with two different hashes, for the same claim —
which reads like one of them was invented. They are one run now, and re-synced
from a single `make repro` at the freeze. CI runs `scripts/repro.sh` on every pull
request and every push to `main`, so a commit that stops reproducing fails there
rather than being discovered by a judge.

## Two directories, not one

The hackathon's FAQ sets the bar at "same machine, same toolchain, build twice",
which same-directory-twice already meets. This does more, because the absolute
build path is the single thing most likely to leak into a binary — it ends up in
panic messages.

So the second directory is deliberately longer than the first. If the path were
leaking, the two binaries would differ in length before they differed in content,
and the check would fail loudly instead of passing by luck.

## The three settings

```sh
SOURCE_DATE_EPOCH=1787940000 CARGO_INCREMENTAL=0 \
RUSTFLAGS="--remap-path-prefix=$PWD=/build -C debuginfo=0" \
cargo build --release --locked --offline
```

`SOURCE_DATE_EPOCH` pins anything that would otherwise embed a build time. The
value is the hackathon kickoff, 2026-08-28 18:00 UTC.

`CARGO_INCREMENTAL=0` because incremental artifacts are not deterministic.

`--remap-path-prefix` is what makes two directories produce one binary. `-C
debuginfo=0` drops the rest of the path leakage that lives in debug info.

## How the check works

`scripts/repro.sh` does not copy the working tree. It runs `git archive HEAD` into
each directory, so both builds see exactly the committed state and an untracked
file cannot make the check pass or fail for the wrong reason.

Then it builds each with `--locked --offline`, hashes
`target/release/stranger` with `sha256sum`, and compares.

## When it fails

The script says where to start:

```text
DIFFER
Bisect order: path leakage in panic messages, then build-id, then
incremental artifacts. Compare with:
  cmp -l A/target/release/stranger B/target/release/stranger | head
```

Path leakage first because it is the most common and the reason the two
directories have different lengths.

## Why an empty manifest makes this easy

Most of what breaks reproducible builds in Rust is dependencies: a build script
that reads the clock or the hostname, a crate that embeds its own path, a
`proc-macro` whose output depends on iteration order. There are none here. There
is also no `build.rs` in this crate and no code generation, so the only inputs are
the source, the corpus text files, and the compiler.

CI runs `./scripts/repro.sh` on every push, so a change that breaks determinism
fails the build rather than being discovered later.

```console
$ make repro
```
