# fixtures

Real lockfiles from real projects, used as test data. **Data, not code** — nothing
here is compiled, linked, or shipped in the binary. Disclosed anyway, in STDLIB.md
as well as here, because over-disclosing costs nothing.

All were screened on collection for private registry hosts, `_auth` / `authToken` /
`password` / `apikey` strings, and `user:pass@` URLs. All clean; every npm scope
present is public.

| file | packages | ecosystem |
|---|---|---|
| `npm-xs.package-lock.json` | 37 | npm, lockfileVersion 3 |
| `npm-s.package-lock.json` | 405 | npm, lockfileVersion 3 |
| `npm-m.package-lock.json` | 582 | npm, lockfileVersion 3 — 3 `link: true`, 3 workspace directories |
| `npm-l.package-lock.json` | 754 | npm, lockfileVersion 3 |
| `npm-xl.package-lock.json` | 1390 | npm, lockfileVersion 3 — 7 `link: true`, 7 workspace directories, 184 nested, 9 install scripts. The benchmark fixture. |
| `cargo-s.Cargo.lock` | 124 | cargo v4 — 1 workspace member, 8 version-qualified dependency strings |
| `cargo-m.Cargo.lock` | 723 | cargo v3 — 15 workspace members, 19 git dependencies, 500 version-qualified |
| `cargo-l.Cargo.lock` | 944 | cargo v4 — 93 workspace members, 597 version-qualified. The workspace fixture. |
| `poetry-s.poetry.lock` | 54 | poetry |
| `poetry-m.poetry.lock` | 233 | poetry |
| `uv-m.uv.lock` | 250 | uv |
| `reqs-xs.requirements.txt` | 12 | pip — every line a bare name, no constraint anywhere |
| `reqs-s.requirements.txt` | 23 | pip |
| `pnpm-l.pnpm-lock.yaml` | 850 | pnpm, lockfileVersion 9 — one importer, 850 snapshots, 1,851 edges, 42 `hasBin`, 3 `deprecated` |
| `gomod-m.go.mod` | 174 | go 1.25.7 — 124 `// indirect`, 26 pseudo-versions, 2 `+incompatible`, three `require` blocks |
| `gomod-xs.go.mod` | 6 | go 1.24 — one single-line `require`, one `retract` block |
| `hostile.package-lock.json` | 5 | npm, lockfileVersion 3 — **written to attack the reader of the report.** See below. |

Neither go fixture contains a `replace`, an `exclude` or a `toolchain`, so those
three are tested against hand-written input in `tests/gomod.rs` — handled,
unmeasured, the same status as the third shape of a Cargo dependency string. The
`retract` block in `gomod-xs` is the one that earns its place: its two lines are
bare versions, and a reader that skipped forward instead of consuming them would
report `v1.4.1` as a module.

Names describe shape, not origin. A directory called `some-company-console/` in a
public repo publishes what someone's private product is built on; `npm-m` does not.

## The poisoned pair

`poisoned.package-lock.json` is `npm-l` with three known-bad names inserted by
hand as root dependencies: 754 entries become 757. `poisoned.requirements.txt` is
not derived from anything — it is six lines written from scratch. They are the
demo and the regression test for the slopsquat rule.

This file said for most of the weekend that the pip fixture was `reqs-s` with
names inserted. It is not. `reqs-s` has 23 lines, every one exactly pinned; the
poisoned file has 6 and shares exactly one of them (`numpy`, and there it carries
no constraint at all). Poisoning `reqs-s` would have meant 23 clean lines around
2 bad ones, which is a worse demo and a slower test than 6 lines that are all
load-bearing.

npm, all three inserted as **root** dependencies with no parent:

| name | nearest real name | why it is here |
|---|---|---|
| `expres` | `express` | one deletion |
| `lodahs` | `lodash` | one **transposition** — Damerau distance 1, Levenshtein distance 2 |
| `chalck` | `chalk` | one insertion |

`lodahs` is why `distance.rs` implements Damerau-Levenshtein rather than plain
Levenshtein — but not for the reason this file gave for most of the weekend. It
said the plain metric needs a threshold of 2 to reach `lodahs`, and that such a
threshold "catches half the registry". The shipped threshold *is* 2. Levenshtein
is pointwise greater than or equal to Damerau, so Damerau-at-k is always the more
permissive of the two, and at k = 2 the plain metric is strictly the tighter one:
against `corpus/npm.txt` it returns 1 candidate for `lodahs` where Damerau returns
3 (`lodash`, `loadjs`, `loodash`).

What Damerau actually buys is the reported distance. It scores the transposition
1 rather than 2, which changes the `d=` in the finding, the tie-break among
candidates, and — the part that would matter — the threshold you could drop to.
At k = 1 the two disagree completely: Damerau finds `lodash`, plain Levenshtein
finds nothing. Measured across every name the rule fires on in every fixture in
this directory, at k = 2 the two metrics fire on **exactly the same set**.
`tests/distance.rs::damerau_changes_the_distance_not_which_names_fire` holds both
halves of that.

The real `express` is still in this tree, one entry above the fake `expres`. A rule
that flags the typo without flagging its legitimate neighbour is the actual bar.

pip, in `poisoned.requirements.txt`:

All six lines, because in a file this short every one is doing a job:

| line | nearest real name | note |
|---|---|---|
| `requests==2.31.0` | — | real, pinned, and quiet. The control. |
| `urllib3>=1.26` | — | unpinned, which is its own finding |
| `python-dateutils==2.9.0` | `python-dateutil` | one insertion |
| `requests-http==1.0.2` | `requests-html` | two edits — see the correction below |
| `flask~=3.0` | — | unpinned, compatible-release form |
| `numpy` | — | no constraint at all |

Which is 2 hallucination findings and 3 unpinned ones, and one line that should
produce neither.

`requests-http` was put in as a name the typo rule should **not** fire on — the
theory being that a hallucinated name which is not a near-miss of a real one is a
different problem, and one rule should not pretend to catch both.

**That was wrong, and the tool found the error.** `requests-http` is two edits
from `requests-html`, which is a real PyPI package with about 14,000 lines of
README behind it. So the rule fires, and it is *right* to fire: `requests-http`
does not exist and never has. The finding is a true positive. What was wrong was
my reasoning about why it would stay quiet — I had not checked whether a real
neighbour existed, and it does.

Left in place, reclassified, because a fixture that corrected a claim in its own
documentation is worth more than one that confirmed it.

## The hostile one

`hostile.package-lock.json` is not from a real project and is not poisoned in the
slopsquat sense. It is five entries whose *strings* are written to attack whoever
reads the report, because a lockfile is a file written by strangers and the human
renderer prints its contents straight to a terminal:

| entry | what it carries |
|---|---|
| `lodahs` | version `1.0.0\x1b[2K\x1b[1A\x1b[2K\x1b[1A\x1b[2K\r` — erases the two report lines above it |
| `chalck` | an SGR sequence in the `name` field, and `hasInstallScript` |
| `csi` | `\u{9b}31m` — a one-byte CSI wherever Latin-1 is still decoded |
| `bell` | bare BEL, backspace, newline, tab, NUL and DEL |
| `aaa…` (300 chars) | one name longer than any terminal is wide |

Before `term::sanitize`, scanning this file made the `HALLUCINATION RISK` heading
and the first finding scroll out of existence while the process still exited 1 —
a finding deleted by the file it was a finding about. `\x1b[2J` cleared the
screen outright. The escape bytes also counted as display columns, so every row
after the first was out of alignment.

The file is legal JSON throughout: every escape is written in `\uXXXX` form, so
nothing here tests the parser. It tests the renderer. Two tests in
`tests/cli.rs` hold it — one asserting no control character reaches stdout in any
mode, one asserting the detail column stays square.

## Measured, not remembered

Every npm count in the table above is `jq '.packages | length - 1'`, every cargo
count is `grep -c '^\[\[package\]\]'`, every pip count is
`awk 'NF && $0 !~ /^[ \t]*#/'`, and every go count is
`grep -cE '[^[:space:]]+\.[^[:space:]]*[[:space:]]+v[0-9]'` — a module path and
a version on one line, which is deliberately not the same as "an indented line",
because that would also count the versions in a `retract` block. All run against
the file in this directory. They are here because the numbers in the README have
to be reproducible by someone who is not me, and because the notes I collected before
the window said npm-xl held 1,391 entries. It holds 1,390.

The same thing happened again with `reqs-xs`, which this table said held 11
requirements until `tests/pip.rs` counted 12. And a third time with the two claims
above it: that the pip fixture was poisoned `reqs-s`, and that `npm-m` has three
workspace members.

The workspace one is the more interesting mistake, because both numbers are real
and they count different things. npm writes a monorepo member twice — once as the
directory (`apps/admin`) and once as a `link: true` entry under `node_modules`
pointing at it. Three of each, so `link: true` is 3 and the report's header says
`6 workspace`. Neither number is wrong; this file just never said which one it
was quoting. `npm-xl` is 7 and 14 by the same arithmetic.
