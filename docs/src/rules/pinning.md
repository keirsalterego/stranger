# Unpinned requirements

An unpinned requirement is not a vulnerability. It is the mechanism by which
somebody else's vulnerability reaches you without anybody changing a file: the
compromised release ships, `pip install -r requirements.txt` runs in CI, and the
diff that introduced it is empty.

Every published pip supply-chain incident has that shape, and it is the reason a
rule about punctuation is worth writing.

```console
$ ./target/release/stranger scan -v fixtures/poisoned.requirements.txt

  poisoned.requirements.txt 6 packages   (6 direct · 0 transitive)

  ⚠  HALLUCINATION RISK     2
     python-dateutils@2.9.0   not in corpus · d=1 from "python-dateutil" · root-only, no parent
     requests-http@1.0.2      not in corpus · d=2 from "requests-html" · root-only, no parent

  ⚠  UNPINNED               3     no exact version recorded
     flask                    ~=3.0 · capped at the major, still floats below the cap
     numpy                    no version specifier · resolves to whatever is newest at install time
     urllib3                  >=1.26 · a range, so the file does not say what installs

  ·  INSTALL SCRIPTS        — no signal in this format

  risk 79/100    9ms    third-party deps used to compute this: 0
```

The file behind that:

```console
$ cat fixtures/poisoned.requirements.txt
requests==2.31.0
urllib3>=1.26
python-dateutils==2.9.0
requests-http==1.0.2
flask~=3.0
numpy
```

`requests==2.31.0` is exact and produces nothing.

## Three severities, and the ranking is a ranking of how much future gets in

| specifier | pin | severity | why |
|---|---|---|---|
| `numpy` | unconstrained | high | no bound in either direction |
| `>=1.26`, `<2`, `!=1.5` | range | medium | open-ended in at least one direction |
| `~=1.2`, `==1.2.*` | compatible | low | capped at the major, floats below the cap |
| `==2.31.0`, `===2.31.0` | exact | — | not a finding |

**Unconstrained** is high. `pip install numpy` today and the same command in March
install different programs, and there is nothing in the repository that records
which one you tested.

**A range** is medium. `>=1.0` is the common case and it is open above: every
release the maintainer has not published yet already matches. `<2` and `!=1.5` are
open below instead, which is a smaller window but the same class of answer — the
file does not say what installs. One notch under unconstrained because at least
one end is written down.

**Compatible** is low. `~=1.2` caps the major, so a hostile 2.0 cannot arrive.
That is a real reduction, and it is not a pin: the compromised releases that
actually happened were patch releases of a package people already trusted, and
every one of those still matches.

Nothing here is critical. An unpinned dependency is a way to be compromised later,
not evidence of being compromised now. Critical is reserved for
[slopsquat](../detection/rule.md), where the finding is a name that should not
exist.

## It fires on PyPI only, and in practice on one PyPI format

npm, cargo and go all record a resolved version, so every entry those readers
produce is `Pin::Exact` and there is nothing to say. Firing on them would mean
either a rule that never triggers or a rule that has started guessing. The rule
returns immediately on any non-PyPI tree.

`poetry.lock` and `uv.lock` are PyPI, so the rule does run over them — and finds
nothing, because those two readers set `Pin::Exact` on every entry they build and
`Pin::Exact` is the arm this rule skips. A lockfile records one resolved version
per package; there is no specifier left to classify.

So the rule walks 233 `poetry-m` entries and 249 `uv-m` entries and returns an
empty list from both. That is a rule doing nothing rather than a rule being
switched off, and it is why the [limits grid](../limits.md) reads `never` for
poetry and uv but not for `requirements.txt`.

## A direct reference is unconstrained

```text
pkg @ https://host/pkg.whl
```

A direct reference names bytes rather than a version, and the bytes at a URL are
whatever the host serves next time. So it classifies as unconstrained, with no
version recorded, which is the honest reading.

## The finding has no version

`version` is empty in the JSON for every pinning finding except none — if a
requirement had an exact version it would not be a finding. That is how a consumer
tells these apart:

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.requirements.txt | jq -c '.findings[] | select(.rule=="pinning")'
{"rule":"pinning","severity":"low","package":"flask","version":"","detail":"~=3.0 · capped at the major, still floats below the cap"}
{"rule":"pinning","severity":"high","package":"numpy","version":"","detail":"no version specifier · resolves to whatever is newest at install time"}
{"rule":"pinning","severity":"medium","package":"urllib3","version":"","detail":">=1.26 · a range, so the file does not say what installs"}
```

The specifier is quoted verbatim in `detail` because the finding has to be
arguable. `>=1.26` is something you can check against the file; "unpinned" is
something you have to take on trust.

## What it cannot see

Whether the range is *deliberate*. A library publishing to PyPI is supposed to
declare ranges; an application deploying from a `requirements.txt` is not. The
file does not say which one it belongs to, and this rule does not guess.

It also cannot see the resolved versions that a real deployment used — not from
this file. All three of the files that do record them are readable, so the answer
is to point `stranger` at one of those instead.

`pip freeze` writes a `requirements.txt` with `==` on every line, which is the
same reader and produces no pinning findings at all:

```console
$ mkdir -p /tmp/freeze
$ printf 'flask==3.0.0\nnumpy==2.1.0\nurllib3==2.2.1\nrequests==2.31.0\n' > /tmp/freeze/requirements.txt
$ cat /tmp/freeze/requirements.txt
flask==3.0.0
numpy==2.1.0
urllib3==2.2.1
requests==2.31.0
$ ./target/release/stranger scan /tmp/freeze

  requirements.txt         4 packages   (4 direct · 0 transitive)

  no findings
  ·  INSTALL SCRIPTS        — no signal in this format

  risk 0/100    0ms    third-party deps used to compute this: 0
```

That still records no graph, so the [detection rule](../detection/rule.md) stays
on two clauses. [`poetry.lock` and `uv.lock`](../formats/poetry-uv.md) record
both the versions and the graph, and are the file to keep if you get to choose.

```console
$ ./target/release/stranger scan -v fixtures/reqs-xs.requirements.txt
```
