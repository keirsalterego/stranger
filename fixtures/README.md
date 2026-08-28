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
| `npm-m.package-lock.json` | 579 | npm, lockfileVersion 3 |
| `npm-l.package-lock.json` | 754 | npm, lockfileVersion 3 |
| `npm-xl.package-lock.json` | 1391 | npm, lockfileVersion 3 — the benchmark fixture |
| `cargo-s.Cargo.lock` | 124 | cargo |
| `cargo-m.Cargo.lock` | 723 | cargo |
| `cargo-l.Cargo.lock` | 944 | cargo |
| `poetry-s.poetry.lock` | 54 | poetry |
| `poetry-m.poetry.lock` | 233 | poetry |
| `uv-m.uv.lock` | 250 | uv |
| `reqs-xs.requirements.txt` | 11 | pip |
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
| `requests-http==1.0.2` | — | not close to anything; caught as unpinned/unknown, not as a typo |
| `urllib3>=1.26` | — | unpinned, which is its own finding |
| `numpy` | — | no constraint at all |

`requests-http` is in there deliberately as a name the typo rule should **not**
fire on. A hallucinated name that is not a near-miss of a real one is a different
problem, and pretending one rule catches both would be a lie the ablation table
would expose anyway.
