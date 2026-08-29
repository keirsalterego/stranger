# Trivial packages

Dependencies whose whole job is one expression. `left-pad` was one of these, and
so was `event-stream`.

```console
$ ./target/release/stranger scan -v fixtures/npm-xs.package-lock.json

  npm-xs.package-lock.json 37 packages   (1 direct · 36 transitive)

  ⚠  TRIVIAL                4     (10.8% of third-party)
     es-errors@1.3.0          one expression, one publisher · inlining it removes an account from your build
     gopd@1.2.0               one expression, one publisher · inlining it removes an account from your build
     has-symbols@1.1.0        predicate-shaped, resolves nothing · size not measured, see rule docs
     hasown@2.0.4             one expression, one publisher · inlining it removes an account from your build

  risk 9/100    13ms    third-party deps used to compute this: 0
```

## It does not measure triviality

Start here, because everything else on this page depends on it.

A `package-lock.json` entry holds `version`, `resolved`, `integrity`, `license`,
`engines` and the dependency lists. There is no unpacked size, no file count, no
export list, no line count anywhere in the format. All of that is in the tarball,
the tarball is on the registry, and `stranger` does not fetch.

So this rule recognises *names*. Nothing it prints should be read as though it had
measured anything.

## Two clauses, and they are different in kind

**A hand-written list.** Two dozen packages whose published purpose is a single
expression or a re-export of a builtin — `isarray`, `is-number`, `left-pad`,
`object-assign`. Picked by hand, which makes the boundary somebody's judgement
rather than a threshold. Against a registry holding millions of names, two dozen
is nothing. That is the honest size of the clause, and there is no version of it
that is not a list somebody wrote.

**Shape.** A name that reads as a predicate (`is-…`, `has-…`, scope stripped)
*and* that resolves no dependencies of its own. Both halves come out of the
lockfile. The second half is what stops it firing on `is-glob` and
`has-tostringtag`, which turned out to need help.

The `detail` string keeps the two apart, because they deserve different amounts of
trust:

```text
one expression, one publisher · inlining it removes an account from your build
predicate-shaped, resolves nothing · size not measured, see rule docs
```

## How clause 2 is wrong

It has no idea how long a file is.

`is-callable` is dozens of lines of edge cases around one `typeof`. `is-docker`
reads `/proc` and memoises the answer. Both are predicate-shaped, both resolve
nothing, both are reported, and neither is a one-liner. That is the false-positive
mode, and it is not the exception — it is a good share of what clause 2 finds on a
real tree.

A hit is worth twenty seconds of attention. It is not a verdict.

## It under-reports at least as badly

`function-bind`, `wrappy` and `util-deprecate` are in the same weight class as
anything on the list and are not on it, because nobody is going to claim to have
read them all. Clause 2 is blind to any micro-package that depends on another
micro-package — `once` needs `wrappy` — and to every one that is not named like a
predicate, which is most of them.

## Why low

None of this is a vulnerability. It is a count of publishers who can push straight
into your build, for code you could have inlined. `left-pad` and `event-stream`
were both packages this size, so the count is worth having. It is never urgent,
and it collapses to a count unless you pass `-v`.

## The percentage

```text
  ⚠  TRIVIAL                29    (2.1% of third-party)
```

The denominator is the header's own package count — third-party entries only.
The rule never looks at a first-party package, so a workspace member cannot be a
hit, and counting one in the denominator would print a share of a population the
numerator was never drawn from.

That was the bug for most of the weekend: the percentage divided by every entry
in the lockfile while the header divided by third-party ones. On `npm-m` it read
17/582 where the header's own numbers say 17/576 — 2.9% against 3.0%. Under a
tenth of a point on every fixture here, which is exactly why it survived so long.
`tests/cli.rs` now derives both numbers from one line of output and fails if they
ever disagree again.

## Duplicates collapse

npm nests the same version of `is-extendable` under two different parents in
`npm-xl`. Two install hooks would be two events, so
[install scripts](install-scripts.md) reports both. Two copies of one expression
are one fact, so this rule prints it once. Different versions still count
separately — `is-docker` at 2.2.1 and 3.0.0 are two findings, and
[version drift](drift.md) is the rule that cares that both exist.

```console
$ ./target/release/stranger scan --format json fixtures/npm-xl.package-lock.json | jq -r '.findings[] | select(.rule=="trivial") | .detail' | sort | uniq -c
```
