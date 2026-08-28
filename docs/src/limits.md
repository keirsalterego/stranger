# Limits

What this tool does not do, stated plainly. A hidden limitation reads as an
oversight.

## Integrity hashes are never verified

Every npm entry carries an `integrity` field:

```console
$ jq -r '.packages | to_entries[] | select(.value.integrity != null) | "\(.key) \(.value.integrity)"' fixtures/poisoned.package-lock.json | head -1
node_modules/@alloc/quick-lru sha512-UrcABB+4bUrFABwbluTIBErXwvbsU/V7TZWfmbgJfbkwiBuziS9gxdODUyuiecfdGQ85jglMW6juS3+z5TsKLw==
```

Rust's standard library contains no cryptography — no SHA-512, no SHA-256, no
hashing primitive suitable for this at all. Implementing SHA-512 by hand to
compare against a hash whose subject is a tarball the tool never downloads would
be theatre.

So the reader records whether an `integrity` field is *present*, never whether it
is correct. Today nothing even reports the presence: `has_integrity` is parsed
and no rule consumes it. A clean scan says nothing whatsoever about whether the
bytes you are about to install match the hash beside them.

This is first on the page because it is the limitation most likely to be
misread as a guarantee.

## `hasInstallScript` is a bare boolean

lockfileVersion 3 records that code runs at install time. It does not record
what that code is, where it comes from, or what it touches:

```console
$ jq '.packages["node_modules/esbuild"] | {version, hasInstallScript}' fixtures/npm-xl.package-lock.json
{
  "version": "0.28.1",
  "hasInstallScript": true
}
```

`npm-xl` has 8 such entries out of 1,390, excluding the root project's own,
which is your build rather than a supply-chain signal. The flag is parsed and
stored. No rule consumes it in this build — `Rule::InstallScript` exists in the
enum and `src/rules/scripts.rs` is a placeholder — so install scripts never
appear in output at all.

## One rule, one format

`slopsquat` is the only rule implemented. `trivial`, `install-script`, `drift`
and `pinning` are declared in the rule enum with report headings and ids, and
their modules are one-line placeholders. They emit nothing.

`package-lock.json` at lockfileVersion 2 or 3 is the only format read. The
repository carries Cargo, poetry, uv, pnpm and pip fixtures; none of them have a
reader in this build.

A clean scan therefore means one thing: no name in this lockfile is absent from
the corpus, close to a corpus name, and unvouched-for. It is not an audit.

## No Go corpus

`proxy.golang.org` publishes no ranked list of module paths, and module paths are
domains — `github.com/spf13/cobra`, not `cobra`. Edit distance over a domain is a
different problem with different failure modes, and a corpus assembled by
guessing would produce confident nonsense.

So the Go corpus is empty, deliberately, and the detection rule can never fire on
a Go module. `tests/corpus.rs` asserts that emptiness so it stays intentional.

## Flat formats have no graph

`requirements.txt` records package names and version constraints. It records no
dependency edges at all. Every package in one trivially has in-degree 0, so
clause 3 is vacuous there and the rule degenerates to distance-alone — which the
[ablation table](detection/ablation.md) shows is worth about 0.03 precision once
the corpus starts thinning.

No flat-format reader ships in this build. The point is that when one lands, the
rule that works on npm does not carry over intact, and pretending otherwise would
be the more comfortable lie.

## The corpus is a snapshot

140,066 npm names fetched on 2026-08-28, in one pass. npm accepts thousands of
new names a day. A package published after that date is indistinguishable to
clause 1 from a package that does not exist. See
[False positives](detection/false-positives.md) for what that costs and
[the ablation table](detection/ablation.md) for how it decays.

The corpus is also a list of names that *exist*, harvested from the registry. It
is not a list of names that are *safe*. A typosquat that actually got registered
is in the corpus, passes clause 1, and is never reported.

## The risk score is not a measurement

Severity weights summed and capped at 100: critical 25, high 10, medium 3, low
1. It is not calibrated against anything, because there is nothing honest to
calibrate it against. Comparing two scans of the same project is meaningful.
Comparing two projects is not.

## Flags that do less than they say

`--no-color` parses and the help text mentions `NO_COLOR`, but this build emits
no colour at all — the output above is plain UTF-8 with no escape sequences in
it. The flag is accepted and changes nothing.

`-q` / `--quiet` currently suppresses only the "no lockfile" message. The header
and footer of a normal report are printed regardless.

## Discovery is one filename, one directory deep

`stranger scan <dir>` looks for exactly `package-lock.json` directly inside
`<dir>`. It does not recurse, on purpose — a walk that descends into
`node_modules` and audits four hundred vendored lockfiles is worse than no walk.
The consequence is that the fixtures directory in this repository scans as empty,
because its lockfiles are all renamed:

```console
$ ./target/release/stranger scan fixtures

  no lockfile in fixtures
  looked for: package-lock.json
```

Point at the file to scan a renamed one.

## Parser details worth knowing

JSON numbers are parsed as `f64`. RFC 8259 puts no limit on magnitude or
precision and `f64` does; nothing in a lockfile is a number this tool does
arithmetic on, so the lossy case is unreachable in practice rather than handled.

Duplicate object keys resolve last-one-wins, which RFC 8259 declines to specify.

Nesting deeper than 128 levels is an error rather than a stack overflow. Real
lockfiles nest about ten deep; the deepest thing in the largest fixture here is
7.

```console
$ ./target/release/stranger scan fixtures/npm-xl.package-lock.json
```
