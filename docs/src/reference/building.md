# Building

```console
$ make
cargo build --release --locked
   Compiling stranger v0.1.0 (<your clone>)
    Finished `release` profile [optimized] target(s) in 1.32s
```

Under two seconds cold, because there is nothing to compile except this crate.

## Targets

| target | what it runs |
|---|---|
| `make` | `cargo build --release --locked` |
| `make test` | `cargo test` |
| `make lint` | `cargo clippy --all-targets -- -D warnings` then `cargo fmt --check` |
| `make fmt` | `cargo fmt` |
| `make ablation` | the slow decay table, about two minutes |
| `make clean` | `cargo clean` |

`make bench` and `make proof` are declared but call `./scripts/bench.sh` and
`./scripts/proof.sh`, and `scripts/` is not in the tree yet. They fail with
`No such file or directory`.

## Tests

```console
$ make test
```

44 tests across six files. 43 run by default; the corpus-decay ablation is
`#[ignore]`d because it takes about two minutes.

| file | tests | what it covers |
|---|---|---|
| `tests/json.rs` | 15 | malformed input, surrogate pairs, deep nesting, error positions |
| `tests/npm.rs` | 9 | fixture counts, nested entries, workspace members, refused versions |
| `tests/distance.rs` | 8 | the OSA counterexample, three property tests at 20,000 cases each |
| `tests/corpus.rs` | 8 | sortedness, PEP 503 normalisation, the `lodash` gap |
| `tests/fixtures.rs` | 2 | every npm fixture parses and its count is what it should be |
| `tests/ablation.rs` | 2 | the two tables |

The distance property tests are seeded from the clock so repeated runs explore
different inputs, and the seed is printed on failure so a bad case can be
replayed. The corpus test asserting byte-order sortedness is not decoration:
`binary_search` on an unsorted slice returns a wrong answer quietly, and shell
`sort` is locale-dependent, so the files are generated with `LC_ALL=C` and
checked rather than trusted.

## The ablation

```console
$ make ablation
cargo test --release --test ablation -- --nocapture --include-ignored
...
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 109.06s
```

Prints both tables from [The ablation table](../detection/ablation.md). Release
mode is not optional here — the decay run scans the fixtures ten times against
140,066 names, and a debug build makes it unpleasant.

## Lint

```console
$ make lint
```

Clippy with `-D warnings` across all targets, then a formatting check. Both must
be clean.

## Reproducibility

`rust-toolchain.toml` pins the compiler to 1.98.0 rather than `stable`. Two
reasons: `str::substr_range` and `NumBuffer::format_into` both landed in 1.98
and both are used, and a floating channel makes the binary non-reproducible for
no benefit.

`--locked` makes Cargo refuse to write `Cargo.lock`. With three empty dependency
tables there is nothing it could write, which is the point — a build that
somehow acquired a dependency fails instead of quietly succeeding.

## The book

```console
$ mdbook build docs
$ ./docs/check-links.py
```

`check-links.py` resolves every relative markdown link against the file it
appears in and exits non-zero on the first target that does not exist. mdBook
turns a broken link into a 404 without complaining, so a rotted page looks
exactly like a fine one until somebody clicks it. External URLs are skipped,
because checking those needs the network and this is a repository whose whole
argument is that it does not need the network.

```console
$ make test && make lint
```
