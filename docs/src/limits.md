# Limits

What this tool does not do. A hidden limitation reads as an oversight; a named one
reads as judgement.

The four that matter most are argued in full in the LIMITS section of the
[README](https://github.com/keirsalterego/stranger#limits), which is the canonical
home for them. Restated here in one line each so this page is not misleading by
omission, and then the rest, which the README does not carry.

## The headline four

**Integrity hashes are never verified.** Rust's standard library has no
cryptography of any kind, so the reader records whether an `integrity` field is
present and never whether it is correct.

**`hasInstallScript` is a bare boolean.** Code runs at install time; what that code
is lives in a tarball on the registry, and `stranger` does not fetch. See
[Install scripts](rules/install-scripts.md).

**No Go corpus.** `go.mod` reads — 174 requirements out of `gomod-m`, 50 direct
against 124 `// indirect` — and the name is the part that does not.
`proxy.golang.org` publishes no ranked list and module paths are domains, so the
corpus is empty on purpose and the detection rule can never fire on a Go module.
`tests/corpus.rs` asserts that emptiness so it stays intentional, and
`tests/gomod.rs` asserts the silence. See [go](formats/gomod.md).

**Flat formats have no graph.** `requirements.txt` and `go.mod` record no
dependency edges, so clause 3 is vacuous and the detection rule runs on two
clauses — on `requirements.txt`, at any rate, being the one of the two where it
runs at all. See [pip](formats/pip.md) and, for it costing a real false positive,
[False positives](detection/false-positives.md). `stranger tree` says the file
has no graph rather than reporting the in-degree 0 it would find there.

## Seven formats, five rules, and most pairs are not real

`package-lock.json` (lockfileVersion 2 and 3), `pnpm-lock.yaml` (v9),
`Cargo.lock` (v3 and v4), `poetry.lock`, `uv.lock`, `requirements.txt`,
`go.mod`. Four ecosystems, three shared parsers — JSON, YAML, TOML — with
`requirements.txt` and `go.mod` reading their own lines, and one graph model.

The grid below is the useful limit, because most of it is empty. A rule that
cannot fire on your ecosystem is not protecting you from anything:

| rule | npm | pnpm | cargo | poetry / uv | requirements.txt | go.mod |
|---|---|---|---|---|---|---|
| slopsquat | yes | yes | registry crates only | yes | yes, weakened | never — no corpus |
| install-script | yes | never | never | never | never | never |
| trivial | yes | yes | effectively never | effectively never | effectively never | effectively never |
| drift | yes | yes | yes | yes | not on a well-formed file | never |
| pinning | never | never | never | never | yes | never |

Most of the `install-script` row is one fact repeated. `install_script` is a
field npm has and nobody else records — pnpm does not carry it, `Cargo.lock` says
nothing about `build.rs`, and neither poetry nor uv notes that a package runs
`setup.py`. The reader sets the flag to `false` and each one says so in its module
docs, so a quiet report on those four formats means *not measured*, never *safe*.

`go.mod` is the one cell in that row where `false` is a measurement: the module
system has no install-time hook to record. `go mod download` fetches and unpacks
a zip, and nothing in it runs until you build.

`trivial` is a hand-written list of npm micro-packages plus a predicate-shaped
name heuristic. Nothing stops it running elsewhere and nothing makes it useful
there; on `pnpm-l` it fires 23 times because pnpm packages *are* npm packages.

`pinning` is the mirror image: every other format on this list records an exact
version for every entry, so the rule has nothing to say and never says it.

`slopsquat` on cargo is narrowed on purpose. A crates.io corpus can only speak
about crates.io, so a package the lockfile marks as coming from git or a path
is skipped rather than reported —
[why that matters](detection/false-positives.md). On `go.mod` it is not narrowed
but switched off, because there is no ranked list of module paths for a name to
be absent from and pretending otherwise would make every module a candidate.

A clean npm scan means: no name is absent-from-corpus-and-near-a-real-one-and-
unvouched-for, nothing declares an install script, nothing appears at two
versions, and nothing is a known micro-package. It is not an audit.

## The corpus is a snapshot, and it lists existence, not safety

140,066 npm names, 15,000 PyPI, 5,000 crates.io, fetched on 2026-08-28 in one
pass. A package published after that date is indistinguishable to clause 1 from a
package that does not exist. The [ablation table](detection/ablation.md) measures
how fast that ages.

The corpus is also a list of names that *exist*, harvested from the registries. A
typosquat that actually got registered is in the corpus, passes clause 1, and is
never reported.

## The trivial rule measures nothing

Named here because it is the loudest rule and the least trustworthy. It has no
access to file sizes, line counts or export lists — none of that is in a lockfile
— so it recognises names, using a hand-written list of two dozen and a shape
heuristic. `is-callable` and `is-docker` are both reported and neither is a
one-liner. [Trivial packages](rules/trivial.md) has the argument.

## Discovery matches names, not contents

`stranger scan <dir>` recurses, but it finds a lockfile by its filename. The
seven names in `lock::KNOWN` are the whole list:

```console
$ ./target/release/stranger scan /tmp

  no lockfile in /tmp
  looked for: package-lock.json, pnpm-lock.yaml, Cargo.lock, requirements.txt, poetry.lock, uv.lock, go.mod
```

The match is on the *end* of the filename, so a prefixed copy is still found —
`poisoned.package-lock.json` and `npm-xl.package-lock.json` both scan, which is
why the fixtures directory in this repository works. A file renamed at the other
end does not: `requirements-dev.txt` and `deps.lock` are invisible to a directory
scan, and nothing inspects contents to second-guess that. Point at such a file
directly and the reader is chosen by the same suffix rule, so it stays invisible
there too.

`walk::SKIP` names thirteen directories the walk will not enter — `node_modules`,
`.git`, `target`, `vendor`, `.venv`, `venv`, `__pycache__`, `.tox`,
`.mypy_cache`, `.pytest_cache`, `dist`, `.next`, `.svelte-kit` — and every other
dot-directory is skipped by a separate check, which is why the seven dotted names
on that list are belt and braces. The walk caps at `MAX_DEPTH = 6`, sorts for
determinism, and does not follow symlinks. Auditing four hundred vendored
lockfiles belonging to other people is worse than auditing none, and
`tests/cli.rs::a_directory_scan_skips_vendored_lockfiles` asserts a lockfile
inside `node_modules` is not picked up.

Depth 6 counts directories below the one you named: a lockfile six levels down is
found, and one seven levels down is not. A `dist/` or `.cache/` you actually
wanted audited has to be named as the scan path directly.

## Code that exists and is not used

`src/semver.rs` is a semver comparator with 13 tests, including the prerelease
precedence rules from section 11 that most implementations get wrong. Nothing
calls it. [Version drift](rules/drift.md) compares version strings for equality,
which is all that rule needs, and no other rule asks an ordering question yet.

It is honest as a library and it is not a feature. Nothing in this book describes
behaviour it provides, because it does not provide any.

## Numbers that do not quite line up

The human report prints `risk N/100` and 100 is not a score this tool can
produce. The number is a band for the worst severity — critical is 75 — plus
`24 * n / (n + 8)` for the count at that severity, and that term is below 24 for
every `n`. So the real ceiling is 98, it takes 184 critical findings in one tree
to reach it, and the worst fixture in this repository sits at 81. The `/100` is a
denominator readers expect rather than one the arithmetic produces;
[JSON output](using/json.md) documents the honest range, 0–98.

The workspace count used to be the entry here — the human report printed it and
the JSON object did not. It does now, as `workspace`, so a consumer no longer has
to parse the header line to tell a monorepo from a flat project of the same size.

## The risk score is not a measurement

A band for the worst severity present — critical 75, high 50, medium 25, low 1 —
plus a saturating term for how many findings share it. It is not calibrated
against anything, and there is nothing honest to calibrate it against. Two
projects are comparable at the band and not below it. Two scans of the same
project are comparable outright.

Gate on `--fail-on`, which compares severities, and not on this number.

It used to be the weights summed and capped at 100, which saturated on any real
tree: nine of the sixteen fixtures here scored exactly 100, including both
`poisoned.package-lock.json` and the clean `npm-l` it was built from. That is
fixed, and the number is still a handle rather than a measurement.

## Parser details worth knowing

JSON numbers are parsed as `f64`. RFC 8259 puts no limit on magnitude or precision
and `f64` does; nothing in a lockfile is a number this tool does arithmetic on, so
the lossy case is unreachable in practice rather than handled.

Duplicate JSON object keys resolve last-one-wins, which RFC 8259 declines to
specify.

Nesting deeper than 128 levels is an error rather than a stack overflow, and it
carries a position:

```text
stranger: nesting deeper than 128 at 1:129
```

The margin is wider than that number suggests. Every `package-lock.json` fixture
here nests to exactly 5 containers, `npm-xl` included — the root object, then
`packages`, then one entry, then `peerDependenciesMeta`, then one peer name. npm
writes a flat map keyed by install path rather than a tree, so the depth does not
grow with the size of the project; `npm-xs` at 37 packages and `npm-xl` at 1,376
nest identically.

The pip reader does not follow `-r` includes and drops `--index-url` lines, which
is the more interesting of the two omissions — an extra index is the
dependency-confusion vector. [pip](formats/pip.md) explains why there is nowhere
honest to put it yet.

## Column widths assume ASCII names

Cell width is one column per Unicode scalar, which is wrong for East Asian wide
forms, combining marks and emoji ZWJ sequences. Registry names are ASCII in
practice — npm permits only URL-safe characters, PyPI normalises to `[a-z0-9.-]`,
crates.io to `[A-Za-z0-9_-]` — so it is exact for every name a lockfile can hand
over, and a name with an accent in it still lines up. If a registry that permits
CJK identifiers turns up, this needs a generated width table.

```console
$ ./target/release/stranger scan -v fixtures/npm-xl.package-lock.json
```
