# cargo

`Cargo.lock`, version 3 and 4.

```console
$ ./target/release/stranger scan fixtures/cargo-m.Cargo.lock

  cargo-m.Cargo.lock       708 packages   (34 direct · 674 transitive · 15 workspace)

  ⚠  HALLUCINATION RISK     1
     ksni@0.3.4               not in corpus · d=2 from "jni" · root-only, no parent

  ⚠  VERSION DRIFT          70    same package at 2+ versions in one tree

  risk 77/100    10ms    third-party deps used to compute this: 0
```

`ksni` is a real crate below the top 5,000 and is a false positive —
[False positives](../detection/false-positives.md) has it, and it is the reason
the crates.io corpus's size is a published number rather than an implementation
detail.

Structurally this is the easiest of the three formats: an array of `[[package]]`
tables, no install paths to reproduce, no nesting. What makes it non-trivial is
that its dependency strings are not names.

## The three shapes of a dependency string

Cargo writes the shortest form that is unambiguous and promotes only when it must:

```text
"bytes"                                  name
"winapi 0.3.9"                           name version
"qux 1.0.0 (registry+https://…)"         name version source
```

The second appears when two entries share a name — `cargo-m` has five `hashbrown`s
and three `windows-sys`. The third appears when two entries share a name *and* a
version and differ only in origin.

Counted across all three fixtures, 5,689 dependency strings in total:

| fixture | bare | name+version | name+version+source |
|---|---|---|---|
| `cargo-s` | 251 | 8 | 0 |
| `cargo-m` | 1,723 | 500 | 0 |
| `cargo-l` | 2,610 | 597 | 0 |

The third shape is **not exercised by any real fixture here**. It is implemented
and tested against a hand-written file, and that is the honest status: handled,
unmeasured.

The second shape is very much exercised — 1,105 strings, a fifth of the corpus —
and reading one as a bare name would resolve half of `cargo-m`'s `windows-sys`
edges to the wrong entry. Silently: it produces no parse error, just a graph with
wrong edges, which corrupts the in-degree the
[detection rule](../detection/rule.md) leans on.

The invariant that makes the bare form safe is Cargo's, not this reader's — a bare
name is written only when exactly one entry carries it. That was checked rather
than assumed: across all three fixtures, zero bare names refer to a duplicated
package and zero dependency strings of any shape fail to resolve.

## No `source` means somebody in this repo wrote it

| `source` | what it is |
|---|---|
| `registry+https://…` | crates.io, or another registry |
| `git+https://…#rev` | a git dependency — **not** first-party |
| absent | a workspace member or a `path = "…"` dependency |

There is nowhere to fetch a path dependency from, so Cargo writes no `source` key
at all. That is the whole first-party test, and it is npm's `link: true` rule
wearing different clothes. It matters for the same reason: `cargo-l` is a 944-entry
workspace with 93 members, and if their edges counted as evidence, a hallucinated
crate added to any one of those 93 `Cargo.toml` files would arrive with in-degree 1
and never be looked at. Those edges become roots instead.

A git dependency is deliberately not first-party. Somebody outside this repo wrote
it, and bypassing crates.io is if anything more interesting — but it also means the
crates.io corpus was never asked about it, which is why
[the name rules stay quiet](../decisions.md) on anything whose origin is not the
registry the corpus samples. That fix removed two of `cargo-m`'s three original
findings.

## What the format does not record

**Build scripts.** Cargo runs `build.rs` at compile time — the same
arbitrary-code-execution shape npm's `hasInstallScript` flags — and `Cargo.lock`
records nothing about it. `install_script` is false on every package, so
[install scripts](../rules/install-scripts.md) never fires on a Rust tree.
Inventing a proxy (the `-sys` suffix, say) would produce a confident wrong answer,
which is worse than a blank. A real answer needs the `.crate` archive or the
index's metadata, and both mean fetching.

**dev-dependencies, and optional ones.** `Cargo.lock` distinguishes neither. A
feature-gated dependency that was resolved is written exactly like any other, so
`dev` and `optional` are false on every package — a limitation, not a measurement.
Splitting them out needs `Cargo.toml`, the workspace's, and feature unification,
which is a resolver.

**Checksums on v1 files.** `checksum` on the package table is v2-and-later; v1 kept
them in a `[metadata]` table keyed `"checksum bytes 1.0.0 (registry+…)"`. This
reader does not look there, so a v1 file reads as having no integrity anywhere.
There is no v1 file in the corpus and Cargo has rewritten them on every
`cargo update` since 2019.

`has_integrity` is otherwise exact. 93 of `cargo-l`'s 944 entries have no checksum
and all 93 are the workspace members. `cargo-m` has 34 without one: 15 workspace
members and 19 git dependencies, which have a source and no checksum because a git
revision is its own integrity claim.

## Which rules can fire

Only two. `slopsquat` on registry crates, and `drift` — every entry records an
exact version so `pinning` has nothing to say, `install_script` is never set, and
the trivial rule's name list is npm micro-packages.

```console
$ ./target/release/stranger scan fixtures/cargo-l.Cargo.lock
```
