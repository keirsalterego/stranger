# stranger

A model suggests a package. You paste the name into `package.json`. The name is
not real — nobody has ever published it — and the first person to notice
registers it and ships whatever they like to everyone who runs `npm install`.

`stranger` reads a lockfile and tells you which names in it look invented, which
of them run code when you install them, and which are installed at four
different versions at once. It does not install anything, does not resolve
anything, and never opens a socket.

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json

  poisoned.package-lock.json 757 packages   (35 direct · 722 transitive)

  ⚠  HALLUCINATION RISK     3
     chalck@5.3.0             not in corpus · d=1 from "chalk" · root-only, no parent
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent
     lodahs@4.17.21           not in corpus · d=1 from "lodash" · root-only, no parent

  ⚠  INSTALL SCRIPTS        3     arbitrary code at install time

  ⚠  TRIVIAL                35    (4.6% of tree)

  ⚠  VERSION DRIFT          55    same package at 2+ versions in one tree

  risk 100/100    141ms    third-party deps used to compute this: 0
```

Three planted names, three findings, no false positives. The critical rule lists
every hit; the other three collapse to a count until you ask for more with `-v`.

## The empty manifest

The last field of that output is checkable. `Cargo.toml` has three dependency
tables and all three are empty, so the lockfile of a lockfile auditor is one
package long:

```console
$ cargo tree
stranger v0.1.0 (/home/keir/stranger)

$ grep -c '^\[\[package\]\]' Cargo.lock
1
```

That is the whole dependency graph. No `serde_json`, no `clap`, no `strsim`, no
`toml`, no `owo-colors`. The JSON reader, the TOML reader, the argument parser,
the edit distance, the semver comparator, the terminal handling and the report
writer are all in `src/`, and the test harness is the one Rust ships with, so
there is no dev-dependency escape hatch in use either.
`STDLIB.md` names each crate that was replaced and what was given up.

`#![forbid(unsafe_code)]` sits at the top of both `src/lib.rs` and `src/main.rs`,
and CI fails the build if `cargo tree` ever prints a second line, if `unsafe`
appears in `src/`, or if anything reaches for `Command::new`.

Rust's standard library has no TLS and no HTTP client. A binary with no
dependencies therefore cannot make a network request — the guarantee comes from
the empty manifest, not from anyone's restraint.

## Reading files other tools produced

The hackathon rules forbid shelling out to installed tooling. Their FAQ rules
this design in explicitly:

> Parsing files those tools already produced is fine, because nothing
> third-party ends up in your artifact.

Two conditions attach. It has to be disclosed, and it has to degrade when the
file is not there. Both hold. The corpus of known-real names and the fixture
lockfiles are written up in
[corpus/PROVENANCE.md](https://github.com/keirsalterego/stranger/blob/main/corpus/PROVENANCE.md),
[fixtures/README.md](https://github.com/keirsalterego/stranger/blob/main/fixtures/README.md)
and `STDLIB.md`, and pointing `stranger` at a directory with no lockfile prints
what it looked for and exits 0:

```console
$ ./target/release/stranger scan /tmp/empty

  no lockfile in /tmp/empty
  looked for: package-lock.json, requirements.txt

$ echo $?
0
```

## What it reads today

Two formats: `package-lock.json` at lockfileVersion 2 or 3, and
`requirements.txt`. Five rules, one of which has an idea in it — the
[co-occurrence rule](detection/rule.md), which separates a hallucinated name from
a legitimate sibling using something other than spelling. The rest of this book
is what those things do, what they measure, and where they are wrong.

Start here:

```console
$ make && ./target/release/stranger scan fixtures/poisoned.package-lock.json
```
