# Installing

There is nothing to install. Clone, type `make`, and a binary appears.

```console
$ git clone https://github.com/keirsalterego/stranger
$ cd stranger
$ make
cargo build --release --locked
   Compiling stranger v0.1.0 (<your clone>)
    Finished `release` profile [optimized] target(s) in 1.32s
```

A cold build of the whole thing takes under two seconds, because there is
nothing to compile except this crate.

`make` is `cargo build --release --locked`. The `--locked` matters even with an
empty manifest: it makes Cargo refuse to write `Cargo.lock`, so a build that
somehow needed a dependency would fail rather than quietly acquire one.

The compiler is pinned in `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.98.0"
components = ["clippy", "rustfmt"]
```

Not `stable`. 1.98 is where `str::substr_range` and `NumBuffer::format_into`
landed, and both are load-bearing — the first gives the JSON parser its error
positions, the second does digit grouping in the report without allocating. A
floating `stable` would also make the build non-reproducible for no gain.

Nothing is fetched. There is no `cargo fetch` step because there is nothing to
fetch, which means the build works with the network cable out.

## What you get

```console
$ ls -l target/release/stranger
-rwxrwxr-x 2 keir keir 3602672 Aug 29 00:19 target/release/stranger
```

3.4 MiB, of which about 2.9 MB is the corpus of known-real package names —
140,066 for npm, 15,000 for PyPI, 5,000 for crates.io — compiled in with
`include_str!`. That is the reason the tool works on a plane: no cache
directory, no first-run download, no "corpus not found" failure mode.

Copy the binary wherever you like. It reads the file you point it at and
nothing else; there is no config file and no state directory.

```console
$ make && ./target/release/stranger --help
```
