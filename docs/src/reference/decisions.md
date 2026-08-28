# Decision log

Choices that were not obvious, and what they cost.

## No dependencies, including dev

Three empty tables in `Cargo.toml` and one package in `Cargo.lock`. The
dev-dependency table is empty too, which people miss — Rust ships a test
harness, so there is no need for one, and "zero dependencies except for testing"
is the sort of claim that quietly becomes false.

The cost is a JSON parser, an argument parser, an edit distance and a report
writer written by hand. About 900 lines. The benefit is that the tool's central
claim — no network, nothing third-party — is enforced by the manifest rather
than by discipline, and the build works offline on a cold machine.

## The corpus is compiled in

`include_str!` over three text files, 2.9 MB, resolved at compile time into a
3.4 MiB binary. The alternative is a data file next to the binary or downloaded
on first run.

Compiling it in removes a whole class of failure: no cache directory, no
first-run download, no "corpus not found", no version skew between binary and
data. It is why the tool works on a plane. The cost is binary size and the fact
that refreshing the corpus means rebuilding.

The lists are stored pre-sorted in byte order because that is the order Rust's
`str: Ord` uses and therefore what `binary_search` needs. `LC_ALL=C sort`
produces it; a locale-sensitive `sort` produces something else and breaks
lookups silently, which is why `tests/corpus.rs` asserts sortedness instead of
trusting whoever last regenerated the files.

## Damerau-Levenshtein, unrestricted

The variant is Lowrance-Wagner, not the optimal string alignment version that
most libraries ship under the name "Damerau-Levenshtein". OSA refuses to edit
inside a span it has already transposed, so it scores `CA` against `ABC` as 3
when the true distance is 2, and it fails the triangle inequality — it is not a
metric.

Nothing here currently needs the triangle inequality. The honest version cost
about fifteen extra lines, and a distance function that quietly is not a metric
is fine right up until somebody indexes with it. `tests/distance.rs` has the
counterexample and a 20,000-case property test that OSA would fail.

## Threshold 2

At 3, `lodash` matches `logass`, `nodash`, `loda` and about forty other real
registry entries, and precision on the fixtures collapses. At 2 every
single-character slip is still caught — deletion, insertion, substitution,
transposition — and that is the entire population of typos a model produces.

## The in-degree clause is a flag

`slopsquat::Config::require_no_parent` can be turned off. That is not a supported
mode of the tool; it exists so `tests/ablation.rs` can measure what the clause is
worth. `Config::corpus` is a parameter for the same reason: the corpus is this
rule's largest assumption, and an assumption you cannot vary is one you cannot
measure.

## Edges out of first-party manifests are not edges

They go into `roots`. A dependency edge is evidence a package is real only when a
stranger drew it, and the root `package.json` — plus every workspace member's —
is the thing under audit. Without this, a hallucinated name added to
`apps/desktop/package.json` arrives with in-degree 1 and is never examined.

Both monorepo fixtures here keep almost nothing in the root manifest, so this is
the common case rather than an edge case. See [Monorepos](../cookbook/monorepos.md).

## `peerDependencies` counts as an edge

A peer dependency is a real maintainer writing down a real name, which is
exactly the evidence the rule wants. Counting it can only suppress a finding,
never invent one, so including it moves the rule in the conservative direction.

## Clause order is 1, 3, 2

Clause 1 is a binary search and eliminates all but a couple of dozen names.
Clause 3 is one array index. Clause 2 is a linear scan of 140,066 names. Running
the cheap eliminator first and the expensive test last is why `npm-xl` takes
380 ms instead of considerably longer.

The linear scan looks wrong and is not: it only ever runs for names that already
failed clause 1, and the length filter inside the distance function rejects most
of the corpus before any table is allocated. If the not-in-corpus set ever gets
large, bucket the corpus by length — the ordering by name does no work for that
query.

## Exit 2 for broken, 1 for findings

A CI gate that cannot tell a usage mistake from a finding is a CI gate somebody
turns off. So a bad flag, a missing file and an unreadable lockfile all exit 2,
and only a finding at or above `--fail-on` exits 1.

## lockfileVersion 1 is refused by name

Version 1 has no `packages` map. A reader that looked for one and found nothing
would report a clean project with zero dependencies — the worst possible output
for an auditing tool. Refusing it with an error that says how to upgrade the file
beats mis-reading it.

## A hand-written argument parser

Flags, one subcommand and three exit codes do not need a parser generator, and
this is a repository with no dependency budget anyway. The hand-written version
also gives better errors: it can say what it expected at this position rather
than printing a grammar.

```text
stranger: --format takes `human` or `json`, not `yaml`
```

## Parser positions from `substr_range`

The JSON parser keeps the original input next to the unconsumed remainder and
asks the standard library where one sits inside the other. There is no cursor
struct threaded through thirty functions and no line counter to keep in sync —
the cursor *is* the remainder. Line and column get computed only when an error is
actually being built, which is the one path where rescanning the consumed prefix
does not matter.

## `panic = "abort"`

In the release profile. There is nothing to unwind into; a panic in a
single-shot CLI is a bug, not a condition to recover from.

## Discovery is not recursive

A walk that descends into `node_modules` and audits four hundred vendored
lockfiles is worse than no walk at all. One filename, one directory. Point at a
file to scan anything else.

```console
$ ./target/release/stranger scan --format yaml fixtures/poisoned.package-lock.json; echo $?
```
