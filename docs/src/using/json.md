# JSON output

`--format json` writes one object per lockfile, on one line, followed by a
newline.

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.package-lock.json
{"source":"fixtures/poisoned.package-lock.json","ecosystem":"npm","packages":757,"direct":35,"transitive":722,"risk":75,"elapsed_ms":99,"findings":[{"rule":"slopsquat","severity":"critical","package":"chalck","version":"5.3.0","detail":"not in corpus · d=1 from \"chalk\" · root-only, no parent"},{"rule":"slopsquat","severity":"critical","package":"expres","version":"4.18.2","detail":"not in corpus · d=1 from \"express\" · root-only, no parent"},{"rule":"slopsquat","severity":"critical","package":"lodahs","version":"4.17.21","detail":"not in corpus · d=1 from \"lodash\" · root-only, no parent"}]}
```

## The object

| field | type | what it is |
|---|---|---|
| `source` | string | the path as you gave it, not canonicalised |
| `ecosystem` | string | `npm`, `pypi`, `crates.io` or `go`; only `npm` can appear today |
| `packages` | number | entries in the lockfile, excluding the root project |
| `direct` | number | named by a manifest in this repository |
| `transitive` | number | `packages - direct` |
| `risk` | number | 0–100, weights summed and capped |
| `elapsed_ms` | number | wall time for this scan, as the tool measured it |
| `findings` | array | worst first, then alphabetical by package within a rule |

## A finding

| field | type | what it is |
|---|---|---|
| `rule` | string | `slopsquat`, `install-script`, `trivial`, `drift` or `pinning` |
| `severity` | string | `low`, `medium`, `high` or `critical` |
| `package` | string | the name as the lockfile spelled it |
| `version` | string | may be empty, if the entry recorded none |
| `detail` | string | why this fired, in the rule's own terms |

`detail` is prose meant for a person. Its shape is stable enough to read and not
stable enough to parse — if you need the distance or the nearest name as data,
say so and they can become fields.

Only `slopsquat` produces findings in this build. The other four rule ids exist
in the enum and their modules are placeholders; nothing emits them yet.

## Escaping

The writer escapes what RFC 8259 section 7 requires — `"`, `\`, the three
whitespace shorthands, and anything below `U+0020` as `\uXXXX` — and nothing
else. The `·` separators in `detail` go out as literal UTF-8, which is legal
JSON and what every parser expects. Note the `\"` around `"chalk"` in the output
above: that is a quoted name inside `detail`, correctly escaped.

## No pretty-printer

There is none, and adding one would mean writing a second serialiser to check.
Pipe it:

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.package-lock.json | jq '.findings[0]'
```
