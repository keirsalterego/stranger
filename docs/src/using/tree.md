# Looking at one package

`stranger scan` says a package has no parent. `stranger tree` shows you.

```console
$ ./target/release/stranger tree <pkg> [path]
```

Same walk, same readers, same lockfiles — it just prints the graph around one
name instead of the findings across a file. `path` defaults to `.` and can be a
single lockfile or a directory to walk.

## The one it was built for

The third clause of [the co-occurrence rule](../detection/rule.md) is *nothing in
the lockfile depends on it*. Clauses 1 and 2 you can check by hand: a name is in
`corpus/npm.txt` or it is not, and an edit distance is arithmetic. Clause 3 is a
claim about a graph, and until you can see the graph the only thing to do with it
is believe the report.

```console
$ ./target/release/stranger tree lodahs fixtures/poisoned.package-lock.json

  fixtures/poisoned.package-lock.json   npm · 757 packages

  lodahs@4.17.21   node_modules/lodahs

     depended on by   in-degree 0 · root-only, no parent
                      nothing in this lockfile depends on it. The only
                      reference to the name in the file is the manifest under
                      audit. That is clause 3 of the co-occurrence rule: a
                      hallucinated package is a root dependency, because
                      nothing real has ever heard of it.

     depends on       nothing
```

`root-only, no parent` is the same phrase the finding prints, on purpose. The
report, this page and `src/rules/slopsquat.rs` should not have three ways of
saying one thing.

A real package gives the other answer, out of the same reader and a tree the
same size:

```console
$ ./target/release/stranger tree accepts fixtures/npm-l.package-lock.json

  fixtures/npm-l.package-lock.json   npm · 754 packages

  accepts@2.0.0   node_modules/accepts
     dev-only

     depended on by   in-degree 1
                      express@5.2.1

     depends on       2 direct, to depth 3
     ├─ mime-types@3.0.2
     │  └─ mime-db@1.54.0
     └─ negotiator@1.0.0
```

## The out-edge tree

`--depth` decides how far down it goes, three by default. The cut is announced
on the line it happens at, because a tree that stops without saying so reads as a
package that depends on less than it does:

```console
$ ./target/release/stranger tree qs fixtures/npm-l.package-lock.json --depth 1

  fixtures/npm-l.package-lock.json   npm · 754 packages

  qs@6.15.3   node_modules/qs
     dev-only

     depended on by   in-degree 2
                      body-parser@2.3.0
                      express@5.2.1

     depends on       2 direct, to depth 1
     ├─ es-define-property@1.0.1
     └─ side-channel@1.1.1 · 5 more below, past --depth 1
```

`--depth 0` removes the limit. That is safe to type because the walk expands
each package once and not once per path: a lockfile is a directed graph with
heavy sharing, and printing every path through one is exponential in the depth.
A name whose dependencies were printed earlier comes back as a leaf marked `(*)`,
with a legend under the tree.

Real lockfiles also contain cycles — npm records peer dependencies in both
directions often enough that `a → b → a` is ordinary — and a cycle is marked
where it closes rather than followed:

```console
$ ./target/release/stranger tree eslint fixtures/npm-xl.package-lock.json --depth 0 | grep -B 1 -A 1 cycle
     ├─ @eslint-community/eslint-utils@4.9.1
     │  ├─ eslint@9.39.4 · cycle, back to a name already above it
     │  └─ eslint-visitor-keys@3.4.3
```

## One name, several versions

[Version drift](../rules/drift.md) is one of the four things this tool reports,
so the one thing `tree` must not do is pick a version. npm spells a duplicated
package as a second entry under a nested install path, and every entry gets its
own block:

```console
$ ./target/release/stranger tree ansi-regex fixtures/npm-l.package-lock.json

  fixtures/npm-l.package-lock.json   npm · 754 packages

  ansi-regex@5.0.1   node_modules/ansi-regex
     dev-only

     depended on by   in-degree 1
                      strip-ansi@6.0.1

     depends on       nothing

  ansi-regex@6.2.2   node_modules/ora/node_modules/ansi-regex
     dev-only

     depended on by   in-degree 1
                      strip-ansi@7.2.0

     depends on       nothing

  ansi-regex@6.2.2   node_modules/string-width/node_modules/ansi-regex
     dev-only

     depended on by   in-degree 1
                      strip-ansi@7.2.0

     depends on       nothing
```

The install path is printed next to each version because it is what makes a
second copy a second copy. It also shows up in the in-edge list when two parents
would otherwise be indistinguishable — five packages depending on `ms` can be
three separate entries called `debug@3.2.7`, each nested somewhere different, and
a list that repeated the same label three times with no explanation reads as a
rendering fault.

## Flat formats have no graph

`requirements.txt` records a list, not a graph. There are no edges in the file to
read, so in-degree 0 there is the format declining to say rather than a number
anyone measured — which is the exact confusion clause 3 exists to avoid. It says
so instead of printing a zero:

```console
$ ./target/release/stranger tree tensorflow-gpu fixtures/reqs-xs.requirements.txt

  fixtures/reqs-xs.requirements.txt   pypi · 12 packages

  tensorflow-gpu

     flat format      no graph in this file
                      requirements.txt records no dependency edges at all, so
                      there is no in-degree here to read and no out-edges to
                      walk. Every package in it trivially has in-degree 0,
                      which is why clause 3 is vacuous on a flat file and the
                      rule falls back to two clauses. Point this at a
                      poetry.lock or a uv.lock and there is a graph to look at.
```

`tensorflow-gpu` is the false positive [Limits](../limits.md) uses to make the
same point: a real, deprecated PyPI package that clauses 1 and 2 both fire on,
with no clause 3 available to save it. The fix is a different file, and both of
them are already readable:

```console
$ ./target/release/stranger tree requests fixtures/poetry-s.poetry.lock

  fixtures/poetry-s.poetry.lock   pypi · 54 packages

  requests@2.32.5

     depended on by   in-degree 3
                      cachecontrol@0.14.3
                      poetry@1.8.5
                      requests-toolbelt@1.0.0

     depends on       4 direct, to depth 3
     ├─ certifi@2025.8.3
     ├─ charset-normalizer@3.4.3
     ├─ idna@3.10
     └─ urllib3@2.6.3
```

## When it is not there

A name that is not in the tree is an answer, not a failure. Exit 0, and a list of
what is close, measured with the same Damerau-Levenshtein function and the same
threshold the rule uses. One name in two files is two lines, because the
question `tree` answers is "where is it", and `lodahs` really is in both:

```console
$ ./target/release/stranger tree lodashh fixtures/

  no package named `lodashh` in the 19 lockfiles under fixtures/

  close names that are there:
     lodash                   d=1 · fixtures/npm-s.package-lock.json
     lodash                   d=1 · fixtures/npm-xl.package-lock.json
     lodash                   d=1 · fixtures/pnpm-l.pnpm-lock.yaml
     lodahs                   d=2 · fixtures/hostile.package-lock.json
     lodahs                   d=2 · fixtures/poisoned.package-lock.json

$ echo $?
0
```

## JSON

One object on one line, and it is one object rather than the newline-delimited
stream [`scan` emits](json.md): `scan` answers a question per file, `tree`
answers one question about one name, and "it is in none of these files" is not
something a stream of per-file objects can say.

```console
$ ./target/release/stranger tree lodahs fixtures/poisoned.package-lock.json --format json
{"query":"lodahs","found":true,"lockfiles":1,"depth":3,"occurrences":[{"source":"fixtures/poisoned.package-lock.json","ecosystem":"npm","name":"lodahs","version":"4.17.21","key":"node_modules/lodahs","first_party":false,"direct":true,"records_edges":true,"in_degree":0,"parents":[],"dependencies":[]}],"near":[]}
```

| field | type | what it is |
|---|---|---|
| `query` | string | the name as you typed it |
| `found` | bool | whether `occurrences` is non-empty |
| `lockfiles` | number | how many files were read to answer this |
| `depth` | number | the `--depth` in force; 0 means no limit |
| `occurrences` | array | one per entry that matched, in path then version order |
| `near` | array | populated only when `found` is false |

An occurrence:

| field | type | what it is |
|---|---|---|
| `source` | string | the path as you gave it, not canonicalised |
| `ecosystem` | string | `npm`, `pypi` or `crates.io` |
| `name`, `version`, `key` | string | as the lockfile spelled them |
| `first_party` | bool | a workspace member — your own code |
| `direct` | bool | named by a manifest in this repository |
| `records_edges` | bool | false on `requirements.txt`, true everywhere else |
| `in_degree` | number or null | **null** when `records_edges` is false |
| `parents` | array | `{name, version}`, one per package with an edge in |
| `dependencies` | array | the out-edge tree, nested |

`in_degree` is null and not 0 on a flat format for the same reason the human
output refuses to print a number there. Nobody measured 0.

A dependency node is `{name, version, dependencies}`, plus a `stop` when the walk
stopped at it — `"cycle"`, `"repeat"` or `"depth"`, and `depth` carries `hidden`,
the count of direct dependencies not shown:

```console
$ ./target/release/stranger tree qs fixtures/npm-l.package-lock.json --depth 1 --format json | jq -c '.occurrences[0].dependencies'
[{"name":"es-define-property","version":"1.0.1","dependencies":[]},{"name":"side-channel","version":"1.1.1","stop":"depth","hidden":5,"dependencies":[]}]
```

Two runs over one tree produce the same bytes, human or JSON. There is no timing
in this output at all, so unlike a scan there is nothing to strip before diffing
two of them.

## Flags

| flag | effect |
|---|---|
| `--depth <n>` | levels of out-edges; default 3, `0` for no limit |
| `--format <human\|json>` | as `scan` |
| `--no-color` | as `scan`; also `NO_COLOR` and `CLICOLOR_FORCE` |
| `-q`, `--quiet` | drop the file header and the explanatory prose, keep the numbers |

`--fail-on` and `-v` are `scan` flags and `tree` says so rather than reporting an
unknown option — `tree` produces no findings, so there is nothing to gate on and
nothing collapsed to expand.

```console
$ ./target/release/stranger tree express --fail-on high
stranger: `--fail-on` is a scan flag; tree reports no findings to gate on
$ echo $?
2
```
