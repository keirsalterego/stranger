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
| `make sweep` | every lockfile on this machine, read twice and compared |
| `make clean` | `cargo clean` |

## Tests

```console
$ make test
```

405 tests across 22 files, plus 15 unit tests inside `src/`. Five are
`#[ignore]`d because they are slow — the corpus-decay ablation, the
false-positive-by-length sweep, two deep fuzz campaigns and the JSON differential
run — and each has a `make` target or a script beside it.

The counts below are `grep -c '#\[test\]' tests/*.rs`, so they are re-derivable
rather than remembered. They went stale twice before that was written down.

| file | tests | what it covers |
|---|---|---|
| `tests/cli.rs` | 47 | exit codes, `-q`, `-v`, blind spots, a flag borrowed from a sibling command, and no escapes down a pipe |
| `tests/yaml.rs` | 36 | the subset, literal block scalars, the flow indicators that used to invent a key, and a linearity bound on flow collections |
| `tests/toml.rs` | 34 | the accepted subset, every construct refused with a position, and the header depth that used to abort in `Drop` |
| `tests/json_conformance.rs` | 30 | RFC 8259 clause by clause, each test citing its section |
| `tests/pip.rs` | 29 | PEP 508 shapes, continuations, comments, markers, extras |
| `tests/pnpm.rs` | 25 | the three sections, v6 alongside v9, and that two legal spellings of one lockfile agree |
| `tests/gomod.rs` | 20 | `require` blocks, pseudo-versions, `retract`, `replace`, `exclude`, quoted paths |
| `tests/walk.rs` | 18 | the skip list, depth cap, sorted order, symlinks, and what it could not open |
| `tests/term.rs` | 18 | the four-input colour decision table, column widths, control-character replacement |
| `tests/tree.rs` | 17 | in-degree, out-edges, depth, near names, and the flags a reader set |
| `tests/json.rs` | 17 | malformed input, surrogate pairs, deep nesting, error positions |
| `tests/yarn.rs` | 19 | specifier-keyed edges, the nested blocks an entry carries, Berry and the headerless empty file refused by name |
| `tests/pypi.rs` | 16 | poetry and uv, and clause 3's share under corpus decay |
| `tests/cargo.rs` | 14 | the three shapes of a dependency string, workspace members, git origins |
| `tests/rules.rs` | 12 | all five rules against the fixtures |
| `tests/corpus.rs` | 12 | sortedness, PEP 503 normalisation, length bucketing, the false-positive-by-length table |
| `tests/distance.rs` | 11 | the OSA counterexample, Damerau against plain Levenshtein, three property tests |
| `tests/semver.rs` | 10 | precedence, including prerelease ordering |
| `tests/npm.rs` | 9 | fixture counts, nested entries, workspace members, refused versions |
| `tests/fuzz.rs` | 5 | mutation campaigns over every parser and all eight readers |
| `tests/ablation.rs` | 4 | the three tables |
| `tests/fixtures.rs` | 2 | every npm fixture parses and its count is what it should be |

`tests/cli.rs` drives the built binary rather than the library, because exit codes
and stdout are the actual contract with a CI job and neither is visible from
inside `main`.

The distance property tests are seeded from the clock so repeated runs explore
different inputs, and the seed is printed on failure so a bad case can be
replayed. The corpus test asserting byte-order sortedness is not decoration:
`binary_search` on an unsorted slice returns a wrong answer quietly, and shell
`sort` is locale-dependent, so the files are generated with `LC_ALL=C` and checked
rather than trusted.

## The sweep

```console
$ make sweep
```

The 23 fixtures were chosen partly because they are interesting: a v6 pnpm
lockfile, a yarn entry answering to two specifiers, a `retract` block full of
bare versions. That biases them. They are good at the hard case and say nothing
about the ordinary one, because nobody picked an ordinary file to include.

A developer's disk is a few thousand lockfiles picked by nobody. `make sweep`
runs every one it can find through the reader for it and separates two
outcomes: a *refusal* — a lockfileVersion this tool does not read, a Berry file
wearing yarn's name — is an answer and counts as a pass. A file the reader
could not get through is a bug, and the script exits 1 on one.

On the machine this was written on, 1,484 lockfiles across all eight formats:
8 refused, 0 unread.

Getting there took four fixes, and this is the part worth stating plainly —
every one of the four was in the ordinary case, and not one was reachable from
the fixtures:

| | what refused a valid file |
|---|---|
| [yarn](../formats/yarn.md) | a bare `peerDependencies:` header |
| [yaml](../formats/pnpm.md) | a `deprecated: \|-` block scalar |
| [go.mod](../formats/gomod.md) | a quoted module path — `gopkg.in/yaml.v3` ships one |
| [diff](../using/diff.md) | printed `no change` and exited 1 in the same breath |

Two of them had a comment beside the bug asserting the case did not arise. The
go.mod one said *nothing in the wild does this*, and the counterexample was
already on the disk.

The sweep looks for one failure and the [yarn reader](../formats/yarn.md) had
the opposite one, which no amount of sweeping would have found: an empty file
read as a clean tree of nothing rather than being refused. A pass that only asks
*did the reader get through it* scores that as success. It was caught by feeding
every reader a zero-byte file of its own name and noticing that seven said
*this is not the lockfile its name claims* and one said `risk 0/100`.

### Getting through is not the same as being right

That first pass catches a refusal and misses the worse failure: a reader that
gets through a file and returns the wrong number. The fixtures cannot catch it
either, because the expected counts in `tests/` were produced by this reader —
a test written that way pins the behaviour, it does not check it.

So the second half of `make sweep` counts the same files again in Python, from
each format's spec rather than from `src/`:

| | counted independently as |
|---|---|
| `Cargo.lock` | `[[package]]` blocks; one with no `source` is a workspace member |
| `package-lock.json` | entries under `packages`, minus the root, workspace directories and links |
| `yarn.lock` | entry headers, the only lines at column 0 |
| `pnpm-lock.yaml` | keys at one indent under `packages:`, counted by line rather than parsed |
| `go.mod` | paths under `require`, in both spellings, and under no other directive |
| `poetry.lock` | `[[package]]` blocks — poetry writes none for the root, so all of them |
| `uv.lock` | `[[package]]` blocks, minus the one whose `source` is not a registry |
| the [drift rule](../rules/drift.md) | names holding more than one version, recomputed from the raw file |

Python's own `json` module doing the npm parse is the point of that second row:
the hand-rolled [`src/json.rs`](stdlib.md) and a mature implementation have to
agree on a hundred real files, or one of them is wrong. The three TOML rows are
the same argument aimed at [`src/toml.rs`](../formats/poetry-uv.md).

The pnpm row is line-based on purpose. [`src/yaml.rs`](../formats/pnpm.md) is
the thing under test, and an oracle that parsed the document with the same shape
of parser could agree with it for the same reasons both were wrong. A pnpm
lockfile puts its package keys at exactly one indent under a top-level
`packages:` and nothing else there, which a line scan can see without a YAML
parser at all.

1,373 lockfiles crosschecked, 0 mismatches: 1,058 `Cargo.lock`, 146 `go.mod`,
107 `package-lock.json`, 41 `yarn.lock`, 16 `pnpm-lock.yaml`, and four each of
`poetry.lock` and `uv.lock`.

Seven of the eight readers have a second opinion. `requirements.txt` is the one
that does not, and deliberately: a line in it *is* a requirement, so any oracle
simple enough to be independent gets the answer wrong. A naive one counts 361
requirement lines across the 35 files on this machine where the reader counts
197, and the reader is right — 162 of the difference are `--hash` continuation
lines, which belong to the requirement above them. An oracle that knew that
would be a second copy of the reader, and two copies of one idea agree with each
other for free.

Writing the oracles found a bug in an oracle rather than in a reader, which is
the outcome that makes the exercise worth doing at all. Poetry puts
`source = ["Cython (>=3.0.11,<3.1.0)"]` inside one lxml block's
`[package.extras]`, and the first draft — a regex over the whole block, the same
shape the `Cargo.lock` row uses — read that as the package's own `source` and
called lxml a local project. The reader was right and the second opinion was
wrong. Blocks are cut at their first nested table header now.

The sweep is not in `cargo test` and cannot be: its corpus is whatever happens
to be on the machine, so it is neither fixed nor portable, and a test that
passes because you have no Go modules installed is not a test. It is a separate
target for the same reason `make bench` is.

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

`make bench` writes `bench.md`. It reports **p50 and p99**, not a mean and a
standard deviation: a scan is not normally distributed, the tail is where a CI job
notices, and a mean with an 82 ms sigma — which is what this section used to
quote — says less than either percentile does.

| target | runs | p50 ms | p99 ms |
|---|---|---|---|
| `stranger scan fixtures/npm-xl.package-lock.json` (1,376 third-party packages) | 100 | 233.7 | 248.8 |
| 500 names, all in the corpus | 100 | 9.5 | 10.5 |
| 500 names, none in the corpus | 5 | 10,102.0 | 10,160.6 |

Measured on an Intel Core i5-10200H at 2.40 GHz, 8 cores, governor
**performance** — the governor is in the file because a `powersave` reading is a
measurement of the governor and not of the tool. One fresh process per sample,
five warmup runs first, page cache warm, nearest-rank percentiles with no
interpolation.

The third row is the point of the file. A name in the corpus is answered by a
binary search; a name that is not costs a sweep, and the two 500-name rows are the
same file shape at the same size differing only in whether the names hit. That
cliff is why `corpus::ByLength` exists, and it halved it rather than removing it.

`bench.md` is gitignored on purpose — it is a timing on one machine, not a claim
— so `make bench` gives you your own. It uses `hyperfine` when it is installed and
falls back to a plain timing loop when it is not, which also publishes its own
floor, so `make bench` never answers with `command not found`.

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
$ make docs
checked 37 pages, no broken relative links
checked 37 pages: 127 commands reproduce, 48 not runnable here (`-v` lists them)
```

Two checks, both standard-library-only Python, neither of which ever enters
`Cargo.toml`.

**`check-links.py`** resolves every relative markdown link against the file it
appears in and exits non-zero on the first target that does not exist. mdBook turns
a broken link into a 404 without complaining, so a rotted page looks exactly like a
fine one until somebody clicks it. External URLs are skipped, because checking
those needs the network and this is a repository whose whole argument is that it
does not need the network.

**`check-output.py`** runs every `$ stranger scan fixtures/...` in the book and the
README and compares what the tool actually prints against what the page says it
prints. Elapsed milliseconds are the one thing allowed to differ, because they are
a measurement rather than a claim.

The ones it reports as not runnable are the `make`, `cargo`, `strings`, `jq` and
`grep` lines in the same blocks — a `make ablation` inside a check that runs on
every push would take two minutes to tell you what `make ablation` already tells
you. Every `stranger` invocation against a fixture is checked, and `-v` prints the
rest by name so the gap is a list rather than a number.

It exists because two separate rots got through in one afternoon. The
[risk score](../using/first-scan.md) changed from a capped sum to a band, which
renumbered every published figure — on a branch that did not yet contain the four
new [format pages](../formats/npm.md) being written against the old score on
another branch. Both merged green and three pages went out quoting a number the
tool would not print. Separately, the [co-occurrence rule](../detection/rule.md)'s
own three worked examples had stopped firing entirely when packages gained an
origin, because their hand-written lockfiles carry no `resolved` field.

Neither was catchable by review. Both are caught now, in `ci` rather than in the
docs workflow, because the check needs the release binary.

```console
$ make test && make lint
```
