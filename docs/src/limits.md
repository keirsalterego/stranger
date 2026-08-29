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

**No Go corpus.** `proxy.golang.org` publishes no ranked list and module paths are
domains, so the corpus is empty on purpose and the detection rule can never fire
on a Go module. `tests/corpus.rs` asserts that emptiness so it stays intentional.

**Flat formats have no graph.** `requirements.txt` records no dependency edges, so
clause 3 is vacuous and the detection rule runs on two clauses. See
[pip](formats/pip.md) and, for it costing a real false positive,
[False positives](detection/false-positives.md).

## Two formats, five rules, and not every pair is real

`package-lock.json` at lockfileVersion 2 or 3, and `requirements.txt`. That is the
whole list. The repository carries Cargo, poetry, uv and pnpm fixtures; none of
them has a reader.

Of the five rules, only two can fire on both formats:

| rule | npm | pip |
|---|---|---|
| slopsquat | yes | yes, weakened |
| install-script | yes | never — the format records nothing equivalent |
| trivial | yes | effectively never — the name list is npm micro-packages |
| drift | yes | not on a well-formed file |
| pinning | never — every entry is exactly pinned | yes |

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

## Discovery is one directory deep

`stranger scan <dir>` looks for exactly `package-lock.json` and `requirements.txt`
directly inside `<dir>`. It does not recurse, on purpose — a walk that descends
into `node_modules` and audits four hundred vendored lockfiles is worse than no
walk.

`src/walk.rs` implements the recursive version properly: a skip list for
`node_modules`, `target`, `.venv` and nine more, a depth cap of 6, deterministic
sorted order, and no symlink following. It has eight tests. It is not wired into
the binary. Until it is, the fixtures directory in this repository scans as empty,
because its lockfiles are all renamed:

```console
$ ./target/release/stranger scan fixtures

  no lockfile in fixtures
  looked for: package-lock.json, requirements.txt
```

Point at the file to scan a renamed one.

## Code that exists and is not used

`src/toml.rs` is a TOML subset reader with 26 tests. `src/semver.rs` is a semver
comparator with 13, including the prerelease precedence rules from section 11 that
most implementations get wrong. `src/walk.rs` is above. None of the three is
reachable from `main`.

They are honest as libraries and they are not features. Nothing in this book
describes behaviour they provide, because they do not provide any yet.

## Numbers that do not quite line up

The JSON object has no workspace count, though the human report prints one, and it
is not recoverable from the other fields.

## The risk score is not a measurement

Severity weights summed and capped at 100: critical 25, high 10, medium 3, low 1.
It saturates on any real tree — every fixture here with more than one rule firing
scores 100 — and it is not calibrated against anything. Comparing two scans of the
same project is meaningful. Comparing two projects is not.

## Parser details worth knowing

JSON numbers are parsed as `f64`. RFC 8259 puts no limit on magnitude or precision
and `f64` does; nothing in a lockfile is a number this tool does arithmetic on, so
the lossy case is unreachable in practice rather than handled.

Duplicate JSON object keys resolve last-one-wins, which RFC 8259 declines to
specify.

Nesting deeper than 128 levels is an error rather than a stack overflow. Real
lockfiles nest about ten deep; the deepest thing in the largest fixture here is 7.

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
