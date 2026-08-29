# go

`go.mod`. Like `requirements.txt` it is a manifest rather than a resolver's
answer, and unlike `requirements.txt` that is almost fine.

```console
$ ./target/release/stranger scan fixtures/gomod-m.go.mod

  gomod-m.go.mod           174 packages   (50 direct · 124 transitive)

  no findings
  risk 0/100    0ms    third-party deps used to compute this: 0
```

No findings is the honest answer here rather than a clean bill of health, and
the [last section](#which-rules-can-fire) says why in a table.

## The version on the line is a floor, and usually the ceiling too

Minimal version selection is what makes a manifest readable as a lockfile. Each
`require` names a *minimum*, and the build picks the largest minimum anybody in
the module graph named. Since Go 1.17 `go mod tidy` writes the whole build list
into the file — every indirect module, at the version that was selected — so the
file and the build agree unless somebody hand-edited one of them.

So every entry is `Pin::Exact` and [unpinned requirements](../rules/pinning.md)
has nothing to say. Reporting these as ranges would fire that rule on all 174
entries above and be wrong 174 times.

The caveat is the `go` line. On a `go 1.16` module the same file lists direct
requirements only, and the rest of the tree lives in the module graph, which is
not in this file and not reachable without the network. Both fixtures here are
1.17 or later.

## `// indirect` is the entire graph

There are no edges in this format. A go.mod records *that* a module is needed
transitively and never *through what*, so `edges` is empty, every package has
in-degree 0, and the [detection rule's](../detection/rule.md) third clause is
vacuous here in exactly the way it is on a [`requirements.txt`](pip.md).

What the format does give up is the direct/transitive split, which is real and
which a `requirements.txt` cannot produce at all: 50 direct against 124
`// indirect` in the fixture above.

## What each directive does here

| directive | what stranger does with it |
|---|---|
| `require` | one `Package`, direct unless the line ends `// indirect` |
| `module` | the module being audited — not a dependency, and its absence means this is not a go.mod |
| `replace` | a target starting `./`, `../` or `/` makes the module first-party; any other target takes its origin off the registry |
| `exclude` | parsed, then dropped: it names a version that must *not* be selected, which is the opposite of a dependency |
| `retract`, `go`, `toolchain`, `godebug`, `tool`, `ignore` | consumed, and nothing is read out of them |
| anything else | a syntax error with a line and column |

The difference between *consumed* and *skipped* is the whole reason that fifth
row exists. A retract block holds bare versions, and in the wild it holds
`[v1.11.0, v1.11.2]` ranges as well:

```text
retract (
	v1.4.1 // #218
	v1.4.0 // #218 panic on saveSessionTicket
)
```

A reader that skipped forward to the next line it recognised would read those as
module paths and invent two packages out of a block that names none.

Refusing an unknown directive rather than skipping it is the same call the
[TOML subset](../decisions.md) makes. The go team keeps adding directives —
`toolchain` in 1.21, `godebug` in 1.23, `tool` in 1.24 — and a reader that
quietly ignores what it has not heard of will one day quietly ignore a `require`
spelled slightly wrong.

## go.sum is not read

It was the obvious next file and it earns nothing. Three reasons, in ascending
order of how much they settle it:

- it holds a line for every module version in the **graph**, not the build list,
  because `go mod tidy` keeps hashes for versions that lost the selection — so
  counting packages out of it overstates the tree;
- `go mod tidy` also guarantees a line for everything that *is* in the build, so
  `has_integrity` computed from it would be a constant `true`, and a constant is
  not a signal;
- the field it would populate is presence and never correctness, because the
  standard library has no SHA-256 to check an `h1:` hash with. That is the same
  wall the npm reader hits on `integrity`, written up in
  [Limits](../limits.md).

A second file, opened by a reader that is handed one string, for a column of
`true`.

## Which rules can fire

Effectively none, and the fixtures above report nothing at all.

| rule | on go.mod | why |
|---|---|---|
| slopsquat | **no** | there is no ranked list of module paths to be absent from |
| pinning | no | MVS makes every entry exact |
| install-script | no | the module system has no install-time hook to record |
| drift | no | a module path appears once; two major versions are two paths |
| trivial | effectively never | the list is npm micro-packages — but it matches the *last path segment*, so a module ending `/is-foo` would fire |

Three of those are the format recording nothing the rule reads, one is the
`trivial` list being about a different registry, and the first is a decision:
`proxy.golang.org` publishes no ranked list of modules and a module path is a
domain, so "not in the corpus" would mean "not in a list nobody publishes".
`corpus::names` returns an empty slice for Go and
`slopsquat::scan` stops on its first line, so the rule is off for this ecosystem
by decision rather than by the arithmetic of an empty list. `tests/gomod.rs`
hands it a corpus containing a one-edit neighbour of a real module in the tree
and it still says nothing.

What is left is the tree, the split, and the package list:

```console
$ ./target/release/stranger scan --format json fixtures/gomod-xs.go.mod
{"source":"fixtures/gomod-xs.go.mod","ecosystem":"go","packages":6,"direct":5,"transitive":1,"workspace":0,"integrity":0,"risk":0,"findings":[]}
```

## Syntax errors carry a position

An unterminated block is reported where it opened, not at end of file, because
the opening line is the one worth going to:

```console
$ mkdir -p /tmp/bad
$ printf 'module example.com/m\n\nrequire (\n\tgithub.com/pkg/errors v0.9.1\n' > /tmp/bad/go.mod
$ ./target/release/stranger scan /tmp/bad/go.mod
stranger: /tmp/bad/go.mod: `require (` is never closed at 3:1
```

```console
$ ./target/release/stranger scan -v fixtures/gomod-xs.go.mod
```
