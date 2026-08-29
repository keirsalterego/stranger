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

`lodahs` is the reason `distance.rs` implements Damerau-Levenshtein rather than
plain Levenshtein. Under plain Levenshtein it is distance 2, tied with a large
population of legitimate sibling packages, and the threshold that catches it
catches half the registry with it. Under Damerau it is distance 1 and sits alone.

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

## Measured, not remembered

Every npm count in the table above is `jq '.packages | length - 1'`, every cargo
count is `grep -c '^\[\[package\]\]'`, and every pip count is
`awk 'NF && $0 !~ /^[ \t]*#/'`, all run against the file in this
directory. They are here because the numbers in the README have to be
reproducible by someone who is not me, and because the notes I collected before
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
