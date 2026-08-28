# Building

```console
$ make
cargo build --release --locked
   Compiling stranger v0.1.0 (<your clone>)
    Finished `release` profile [optimized] target(s) in 2.18s
```

Under three seconds cold, because there is nothing to compile except this crate.

## Targets

| target | what it runs |
|---|---|
| `make` | `cargo build --release --locked` |
| `make test` | `cargo test` |
| `make lint` | `cargo clippy --all-targets -- -D warnings` then `cargo fmt --check` |
| `make fmt` | `cargo fmt` |
| `make ablation` | the slow decay table, about two minutes |
| `make bench` | 50 timed runs on the largest fixture |
| `make proof` | regenerate `deps-proof.txt` |
| `make repro` | [build twice, compare hashes](reproducible-builds.md) |
| `make clean` | `cargo clean` |

## Tests

```console
$ make test
```

154 tests across 13 files. 153 run by default; the corpus-decay ablation is
`#[ignore]`d because it takes about two minutes.

| file | tests | what it covers |
|---|---|---|
| `tests/toml.rs` | 26 | the accepted subset, and every construct refused with a position |
| `tests/pip.rs` | 24 | PEP 508 shapes, continuations, comments, markers, extras |
| `tests/term.rs` | 17 | the four-input colour decision table, column widths |
| `tests/json.rs` | 15 | malformed input, surrogate pairs, deep nesting, error positions |
| `tests/semver.rs` | 13 | precedence, including prerelease ordering |
| `tests/rules.rs` | 12 | all five rules against the fixtures |
| `tests/cli.rs` | 10 | exit codes, `-q`, `-v`, and no escapes down a pipe |
| `tests/npm.rs` | 9 | fixture counts, nested entries, workspace members, refused versions |
| `tests/corpus.rs` | 8 | sortedness, PEP 503 normalisation, the `lodash` gap |
| `tests/distance.rs` | 8 | the OSA counterexample, three property tests at 20,000 cases each |
| `tests/walk.rs` | 8 | the skip list, depth cap, sorted order, symlinks |
| `tests/fixtures.rs` | 2 | every npm fixture parses and its count is what it should be |
| `tests/ablation.rs` | 2 | the two tables |

`tests/cli.rs` drives the built binary rather than the library, because exit codes
and stdout are the actual contract with a CI job and neither is visible from
inside `main`.

The distance property tests are seeded from the clock so repeated runs explore
different inputs, and the seed is printed on failure so a bad case can be
replayed. The corpus test asserting byte-order sortedness is not decoration:
`binary_search` on an unsorted slice returns a wrong answer quietly, and shell
`sort` is locale-dependent, so the files are generated with `LC_ALL=C` and checked
rather than trusted.

## The ablation

```console
$ make ablation
cargo test --release --test ablation -- --nocapture --include-ignored
...
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 109.42s
```

Prints both tables from [The ablation table](../detection/ablation.md). Release
mode is not optional — the decay run scans the fixtures ten times against 140,066
names, and a debug build makes it unpleasant.

## Benchmarks

```console
$ make bench
fixture: fixtures/npm-xl.package-lock.json (1383 resolved entries)
cpu: Intel(R) Core(TM) i5-10200H CPU @ 2.40GHz
governor: powersave

Benchmark 1: target/release/stranger scan fixtures/npm-xl.package-lock.json
  Time (mean ± σ):     413.0 ms ±  82.5 ms    [User: 404.8 ms, System: 3.4 ms]
  Range (min … max):   371.2 ms … 660.2 ms    50 runs
```

Uses `hyperfine` when it is installed and falls back to a plain 50-run loop when
it is not, so `make bench` never answers with `command not found`.

## Lint

```console
$ make lint
cargo clippy --all-targets -- -D warnings
    Checking stranger v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.43s
cargo fmt --check
```

Clippy with `-D warnings` across all targets, then a formatting check. Both must
be clean.

## What CI enforces

`.github/workflows/ci.yml` runs the above plus three checks that guard the entry's
central claim, and it runs them *before* anything else:

```yaml
- name: The manifest is empty
  run: |
    test "$(cargo tree | wc -l)" -eq 1
    test "$(grep -c '^\[\[package\]\]' Cargo.lock)" -eq 1
```

Then no `unsafe` anywhere in `src/` (excluding the `forbid(unsafe_code)` attribute
itself, which the naive grep would otherwise match), and no `Command::new`
anywhere — `stranger` reads files, it does not run programs. After that: format,
clippy, tests, an offline build, the full ablation, and the reproducible-build
check.

## Reproducibility

`rust-toolchain.toml` pins the compiler to 1.98.0 rather than `stable`. Two
reasons: `str::substr_range` and `NumBuffer::format_into` both landed in 1.98 and
both are used, and a floating channel makes the binary non-reproducible for no
benefit. [Reproducible builds](reproducible-builds.md) has the rest.

`--locked` makes Cargo refuse to write `Cargo.lock`. With three empty dependency
tables there is nothing it could write, which is the point — a build that somehow
acquired a dependency fails instead of quietly succeeding.

## The book

```console
$ mdbook build docs
$ ./docs/check-links.py
checked 27 pages, no broken relative links
```

`check-links.py` resolves every relative markdown link against the file it appears
in and exits non-zero on the first target that does not exist. mdBook turns a
broken link into a 404 without complaining, so a rotted page looks exactly like a
fine one until somebody clicks it. External URLs are skipped, because checking
those needs the network and this is a repository whose whole argument is that it
does not need the network.

```console
$ make test && make lint
```
