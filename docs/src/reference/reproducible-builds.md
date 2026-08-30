# Reproducible builds

Same commit, same toolchain, two different directories, one binary.

```console
$ make repro
commit:  915951c68705be35816afde598b1a489a7a82b28
rustc:   rustc 1.98.0 (88d9e12ae 2026-08-18)
epoch:   1787940000

build A  /tmp/stranger-repro.162272/a
         4403ffad63d28fbbdd6379b443e3c29456bdee59bfd66c61a3f4dea4fe93993f
build B  /tmp/stranger-repro.162272/b-with-a-deliberately-longer-name
         4403ffad63d28fbbdd6379b443e3c29456bdee59bfd66c61a3f4dea4fe93993f

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

`--remap-path-prefix` rewrites the build directory out of anything that carries
it, and `-C debuginfo=0` drops the rest of the path leakage that lives in debug
info.

Neither turns out to be load-bearing, and the honest thing is to say so rather
than let the flag take credit. Two plain `cargo build --release --locked` runs in
two directories of different lengths, with none of the three settings, produce the
same binary and the same hash as `make repro` does. The reason is that Cargo
compiles the local crate through a *relative* path, so `panic!` locations come out
as `src/main.rs` and there is no absolute path for the remap to rewrite:

```console
$ strings target/release/stranger | grep -E '^src/[a-z]+\.rs$' | head -3
src/tree.rs
src/main.rs
src/toml.rs
```

The absolute paths that *are* in the binary belong to the standard library, and
they point into the rustup toolchain rather than into this repository:

```console
$ strings target/release/stranger | grep -c "$HOME/.rustup"
20
```

Those are identical for every build on one machine and different on another, which
is exactly the boundary the FAQ draws. The settings stay because they are what
stops this from silently ceasing to be true — a `include_str!(concat!(env!("...")))`
or a build script would put the build directory in the binary tomorrow, and the
flag is already there when it does.

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
