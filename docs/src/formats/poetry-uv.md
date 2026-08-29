# poetry and uv

`poetry.lock` and `uv.lock` — the two Python lockfiles that record a graph. One
module reads both, because the piece worth writing exactly once is turning a
dependency's name into an entry, and that is where the whole file can go quietly
wrong.

```console
$ ./target/release/stranger scan fixtures/poetry-m.poetry.lock

  poetry-m.poetry.lock     233 packages   (75 direct · 158 transitive)

  no findings
  risk 0/100    8ms    third-party deps used to compute this: 0
```

```console
$ ./target/release/stranger scan fixtures/uv-m.uv.lock

  uv-m.uv.lock             249 packages   (91 direct · 158 transitive · 1 workspace)

  ⚠  VERSION DRIFT          1     same package at 2+ versions in one tree

  risk 27/100    11ms    third-party deps used to compute this: 0
```

A clean report on a 233-package tree is a real result and is published as one.

## Why these exist when `pip.rs` already reads Python

[`requirements.txt`](pip.md) records no edges, so every package in it has in-degree
0 and the [detection rule](../detection/rule.md)'s third clause eliminates nothing.
That costs a real false positive on `tensorflow-gpu`.

Both formats here record the resolved graph, which is the entire reason to read
them. A reader that produced packages and no edges would leave the rule exactly as
weak as it already was — and
[the ablation](../detection/ablation.md) measures the difference: clause 3 removes
60–75% of candidates on poetry and uv, and structurally zero on `requirements.txt`,
at every corpus size.

## What each one records

| | poetry.lock | uv.lock |
|---|---|---|
| entry | `[[package]]` | `[[package]]` |
| edges | `[package.dependencies]`, keys are names | `dependencies = [ { name = "…" } ]` |
| optional edges | `[package.extras]` — **not read** | `[package.optional-dependencies]` — read |
| hashes | `files` array of `{file, hash}` | `sdist` / `wheels` |
| dev split | yes, per package, in poetry 2.x's `groups` | no |
| the root project | absent | `source = { editable = "." }` |

poetry-m has 112 dependency sub-tables, poetry-s has 20. uv-m has 141 `dependencies`
arrays and 7 optional blocks, and no `[[package.dependencies]]` array-of-tables
anywhere — that shape does not occur.

## Why uv's optional block is read and poetry's extras are not

This looks inconsistent and is not.

uv's optional block holds **resolved** references. `davey` is in `uv-m` because
somebody asked for `discord-py[voice]`, so that is a real install edge.

poetry's extras block is the package's metadata copied verbatim whether anyone
asked or not — 1,049 PEP 508 strings in poetry-m, of which **754 name packages that
are not in the lock at all**. Feeding the other 295 into `edges` would put
non-install edges in the graph, and a package mentioned by somebody's unrequested
`docs` extra would stop looking like a root when it is one.

That costs the detection rule 295 pieces of real "a maintainer has heard of this
name" evidence, and losing evidence makes the rule fire *more*, not less. The
upgrade is a separate `mentioned` set for clauses that want weaker evidence than an
install edge — not a wider definition of `edges`.

## poetry does not record the root project

There is no `[[package]]` for the thing being locked, and its direct dependencies
live in `pyproject.toml`, which is not this file. uv does record it, and its
dependency list is the answer poetry makes you infer — which is why the uv header
prints `1 workspace` and poetry's prints none.

## What neither records

**Install-time code.** No field in either format says a package runs `setup.py` at
install time, so `install_script` is false on every entry and
[install scripts](../rules/install-scripts.md) never fires on a Python tree. There
is no honest proxy — an sdist with no wheel is *suggestive* and is not the same
claim — so nothing is invented. This is a real blind spot relative to npm.

**A dev split, in uv's case.** uv-m has no `dev-dependencies` at all, and group
membership in uv attaches to the edge rather than the package, so `dev` is false on
every uv entry.

## What each one refuses

Both readers check a marker before they read anything, and the marker is the
point. `Cargo.lock`, `poetry.lock` and `uv.lock` are all TOML with a
`[[package]]` array, so the file name is the only thing that says which reader
should get it — and a renamed file would otherwise be read by the wrong one and
silently produce a tree with no edges in it, because cargo writes its
dependencies as strings and these two write inline tables.

So poetry requires `metadata.lock-version` and uv requires `requires-python`,
each of which appears in every file its own tool writes and in nothing else here:

```console
$ mkdir -p /tmp/refuse-py
$ printf '[[package]]\nname = "a"\nversion = "1"\n' > /tmp/refuse-py/x.poetry.lock
$ ./target/release/stranger scan /tmp/refuse-py/x.poetry.lock
stranger: /tmp/refuse-py/x.poetry.lock: no `metadata.lock-version`; this is not the lockfile its name claims to be
```

An entry with no `name` is refused by ordinal, exactly as in
[cargo](cargo.md#what-it-refuses) and for the same reason — the value tree
carries no positions.

**An absent `package` array is deliberately not an error.** A project with no
dependencies gets a lockfile that is a header and nothing else, and refusing that
would be a false alarm about a file that is simply empty. The marker check above
is what makes that safe: the file has already proved it is a poetry or uv
lockfile before the empty array is accepted as meaning zero packages.

## The TOML that makes this possible

Both files, plus `Cargo.lock`, go through one [subset parser](../decisions.md).
It has no date type, and the only reason that is sufficient is that `uv.lock`
stores timestamps as strings:

```toml
upload-time = "2026-03-26T01:21:00.379Z"
```

If uv wrote them as TOML datetimes the parser would refuse the file, loudly, with a
line and column. The only bare integers across all six fixtures are `version` and
`revision`.

poetry writes quoted keys containing dots — `"jaraco.classes" = "*"`. That is one
key whose name contains a dot, not a dotted key; quoting decides, not the dot.
Conflating them silently invents a `jaraco` table.

```console
$ ./target/release/stranger scan -v fixtures/uv-m.uv.lock
```
