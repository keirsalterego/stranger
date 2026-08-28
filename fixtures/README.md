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
| `npm-m.package-lock.json` | 582 | npm, lockfileVersion 3 — 3 are `link: true` workspace members |
| `npm-l.package-lock.json` | 754 | npm, lockfileVersion 3 |
| `npm-xl.package-lock.json` | 1390 | npm, lockfileVersion 3 — 7 `link: true`, 184 nested, 9 install scripts. The benchmark fixture. |
| `cargo-s.Cargo.lock` | 124 | cargo v4 — 1 workspace member, 8 version-qualified dependency strings |
| `cargo-m.Cargo.lock` | 723 | cargo v3 — 15 workspace members, 19 git dependencies, 500 version-qualified |
| `cargo-l.Cargo.lock` | 944 | cargo v4 — 93 workspace members, 597 version-qualified. The workspace fixture. |
| `poetry-s.poetry.lock` | 54 | poetry |
| `poetry-m.poetry.lock` | 233 | poetry |
| `uv-m.uv.lock` | 250 | uv |
| `reqs-xs.requirements.txt` | 12 | pip — every line a bare name, no constraint anywhere |
| `reqs-s.requirements.txt` | 23 | pip |
| `pnpm-l.pnpm-lock.yaml` | 850 | pnpm |

Names describe shape, not origin. A directory called `some-company-console/` in a
public repo publishes what someone's private product is built on; `npm-m` does not.

## The poisoned pair

`poisoned.package-lock.json` and `poisoned.requirements.txt` are `npm-l` and
`reqs-s` with known-bad names inserted by hand. They are the demo and the
regression test for the slopsquat rule.

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

| line | nearest real name | note |
|---|---|---|
| `python-dateutils==2.9.0` | `python-dateutil` | one insertion |
| `requests-http==1.0.2` | `requests-html` | two edits — see the correction below |
| `urllib3>=1.26` | — | unpinned, which is its own finding |
| `numpy` | — | no constraint at all |

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
requirements until `tests/pip.rs` counted 12.
