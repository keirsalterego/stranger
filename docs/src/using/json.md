# JSON output

`--format json` writes one object per lockfile, on one line, followed by a
newline. A directory holding both a `package-lock.json` and a `requirements.txt`
produces two lines rather than an array, so the stream is newline-delimited JSON
and a consumer reads it a line at a time:

```console
$ rm -rf /tmp/mixed && mkdir -p /tmp/mixed
$ cp fixtures/poisoned.requirements.txt /tmp/mixed/requirements.txt
$ cat > /tmp/mixed/package-lock.json <<'EOF'
{
  "name": "mixed",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "mixed", "dependencies": { "expres": "4.18.2" } },
    "node_modules/expres": {
      "version": "4.18.2",
      "resolved": "https://registry.npmjs.org/expres/-/expres-4.18.2.tgz",
      "integrity": "sha512-AA"
    }
  }
}
EOF
$ ./target/release/stranger scan --format json /tmp/mixed | jq -c '{source, packages, findings: (.findings|length)}'
{"source":"/tmp/mixed/package-lock.json","packages":1,"findings":1}
{"source":"/tmp/mixed/requirements.txt","packages":6,"findings":5}
```

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.requirements.txt
{"source":"fixtures/poisoned.requirements.txt","ecosystem":"pypi","packages":6,"direct":6,"transitive":0,"workspace":0,"integrity":0,"risk":79,"findings":[{"rule":"slopsquat","severity":"critical","package":"python-dateutils","version":"2.9.0","detail":"not in corpus · d=1 from \"python-dateutil\" · root-only, no parent"},{"rule":"slopsquat","severity":"critical","package":"requests-http","version":"1.0.2","detail":"not in corpus · d=2 from \"requests-html\" · root-only, no parent"},{"rule":"pinning","severity":"low","package":"flask","version":"","detail":"~=3.0 · capped at the major, still floats below the cap"},{"rule":"pinning","severity":"high","package":"numpy","version":"","detail":"no version specifier · resolves to whatever is newest at install time"},{"rule":"pinning","severity":"medium","package":"urllib3","version":"","detail":">=1.26 · a range, so the file does not say what installs"}]}
```

## The object

| field | type | what it is |
|---|---|---|
| `source` | string | the path as you gave it, not canonicalised |
| `ecosystem` | string | `npm`, `pypi`, `crates.io` or `go`. All four appear — `go.mod` reads; what Go has no corpus for is the [detection rule](../detection/rule.md) |
| `packages` | number | third-party packages; workspace members are excluded |
| `direct` | number | named by a manifest in this repository |
| `transitive` | number | `packages - direct` |
| `workspace` | number | first-party entries set aside; 0 on a non-monorepo |
| `integrity` | number | third-party entries that recorded an integrity field. **Presence, never correctness** — std ships no crypto, so no hash is ever computed. See [Limits](../limits.md) |
| `risk` | number | 0–98; a band for the worst severity plus a term for volume |
| `findings` | array | worst rule first, then alphabetical by package within a rule |

`workspace` is the one header number that cannot be rebuilt from the others.
`packages`, `direct` and `transitive` are all third-party counts, so without it a
monorepo and a flat project of the same dependency count read identically.

## A finding

| field | type | what it is |
|---|---|---|
| `rule` | string | `slopsquat`, `install-script`, `trivial`, `drift` or `pinning` |
| `severity` | string | `low`, `medium`, `high` or `critical` |
| `package` | string | the name as the lockfile spelled it |
| `version` | string | may be empty |
| `detail` | string | why this fired, in the rule's own terms |

`version` is empty in two cases that are worth knowing about. A `drift` finding
is one per name across all its versions, so there is no single version to put
there and the versions are in `detail` instead. A `pinning` finding on a
requirement with no `==` has no resolved version to record at all — that is the
finding.

Findings are ordered by `rules::ORDER` — slopsquat, install-script, trivial,
drift, pinning — and alphabetically by package inside each rule. The order is
stable across runs, so a diff between two scans is a diff and not a reshuffle.

`detail` is prose meant for a person. Its shape is stable enough to read and not
stable enough to parse — if you need the edit distance, the nearest name or the
drifted version list as data, say so and they can become fields.

## Everything is listed

`--format json` ignores the collapsing that the human report does. A rule that
prints as `VERSION DRIFT 76` on a terminal emits all 76 objects here, whether or
not you passed `-v`. Same for `-q`: it changes nothing about the JSON.

```console
$ ./target/release/stranger scan --format json fixtures/npm-xl.package-lock.json | jq '.findings | length'
113
```

## Escaping and colour

The writer escapes what RFC 8259 section 7 requires — `"`, `\`, the three
whitespace shorthands, and anything below `U+0020` as `\uXXXX` — and nothing
else. The `·` separators in `detail` go out as literal UTF-8, which is legal JSON
and what every parser expects. Note the `\"` around `"python-dateutil"` above:
that is a quoted name inside `detail`, correctly escaped.

JSON is never coloured, under any combination of `CLICOLOR_FORCE`, `NO_COLOR` and
TTY. It goes to a program, and a program that has to strip SGR codes out of a
string field will not.

## No pretty-printer

There is none, and adding one would mean writing a second serialiser to check.
Pipe it:

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.requirements.txt | jq '.findings[0]'
```
