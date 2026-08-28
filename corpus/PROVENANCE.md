# corpus provenance

Three lists of package names that are known to exist. **Data, not code.** Nothing
here is compiled as source; it is embedded with `include_str!` and read as text.
Disclosed here and again in STDLIB.md, because the rule is that anything I did not
write this weekend gets disclosed, and a name list is exactly the sort of thing it
would be convenient to forget.

Fetched **2026-08-28**, once, with `curl`, at development time. The binary never
makes a network request — Rust's standard library has no TLS, and `stranger` has no
dependencies, so this is enforced by construction rather than by discipline.

| file | names | source |
|---|---|---|
| `crates-io.txt` | 5,000 | `crates.io/api/v1/crates?page=N&per_page=100&sort=downloads`, 50 pages |
| `pypi.txt` | 15,000 | `hugovk.dev/top-pypi-packages/top-pypi-packages.min.json` |
| `npm.txt` | 140,066 | union of three sources, below |

## npm needed three sources, and that is the interesting part

The obvious approach — sweep the registry search API with every two-letter query,
250 results each — produced 126,702 names and **did not contain `lodash`**.

The reason is that `registry.npmjs.org/-/v1/search` ranks by text relevance, and
for a two-character query that ranking is close to meaningless. The query `lo`
returns `lodash._objecttypes`, `lodash._shimkeys`, `lodash._basecreatecallback`
and 247 more of that shape, while `lodash` itself is nowhere in the page. Adding
`&popularity=1.0&quality=0.0&maintenance=0.0` does not change it. Neither does
`loda`, which also fails to return `lodash`.

That matters more than a missing name. The packages a typo-detector needs most are
precisely the ones people typo, which are precisely the popular ones — and the
sweep is worst exactly there. A corpus missing `lodash` reports `lodash` as a
hallucination.

So `npm.txt` is the union of:

1. **The two-letter sweep**, 676 queries × 250 results → 126,702 names. Good long-tail
   coverage, which is what stops obscure-but-real packages being flagged.
2. **`npm-high-impact@1.13.0` `topDependent`** → 4,687 names, ranked by how many
   packages depend on them.
3. **`npm-high-impact@1.13.0` `topDownload`** → 15,916 names, ranked by downloads.

Both `npm-high-impact` lists were fetched as text from
`cdn.jsdelivr.net/npm/npm-high-impact@1.13.0/lib/`, and only the quoted string
literals were extracted. No JavaScript was executed and none of it ships.

## Normalisation

Names are stored lowercased. PyPI names are additionally normalised per PEP 503 —
runs of `-`, `_` and `.` collapse to a single `-` — because PyPI treats those as
the same project and a `requirements.txt` will contain any spelling of it. Without
that, a separator choice reads as a one-character typo.

crates.io keeps `_` and `-` distinct in display, so those are left alone and the
edit distance covers the confusion.

## Sort order

Sorted with `LC_ALL=C`, which is byte order, which is the order Rust's `str: Ord`
and therefore `binary_search` use. A locale-sensitive `sort` produces a different
order and would silently break lookups; `tests/corpus.rs` asserts sortedness rather
than trusting whoever last regenerated these files.

## Why this is not a dependency

It is a text file of names. It does not execute, it is not linked, it has no
transitive anything, and the tool degrades honestly without it: with an empty
corpus every name is "not in corpus", the second clause finds no neighbour, and the
rule reports nothing rather than reporting everything. `Ecosystem::Go` already runs
in exactly that state on purpose — there is no ranked list of Go module paths, so
the rule never fires on `go.mod`, and README LIMITS says so out loud instead of
shipping a rule that silently does nothing.
