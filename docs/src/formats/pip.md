# pip

`requirements.txt`. It is not a lockfile.

That is the first thing to say about it, because everything else follows. A
`requirements.txt` is the input a resolver takes, not the answer it gives. There
is no resolved version unless somebody typed `==`, and there is no dependency
information at all. It is read here because people commit it and then treat it as
a lockfile.

```console
$ ./target/release/stranger scan fixtures/poisoned.requirements.txt

  poisoned.requirements.txt 6 packages   (6 direct · 0 transitive)

  ⚠  HALLUCINATION RISK     2
     python-dateutils@2.9.0   not in corpus · d=1 from "python-dateutil" · root-only, no parent
     requests-http@1.0.2      not in corpus · d=2 from "requests-html" · root-only, no parent

  ⚠  UNPINNED               3     no exact version recorded

  risk 79/100    15ms    third-party deps used to compute this: 0
```

## The format is flat, and the detection rule pays for it

There are no transitive entries, no nesting, and nothing that says one line needs
another. So every package is a root, `roots` is `0..packages.len()`, and **`edges`
is left empty on purpose**. That is not the reader giving up — the edges are not
in the file to read.

Which makes the detection rule's third clause — *nothing real depends on this
name* — **vacuous on this format**. Every package trivially has in-degree 0, the
clause eliminates nothing, and the rule degenerates to not-in-corpus AND
near-a-real-name. Those two are exactly the half of the conjunction that the
[ablation table](../detection/ablation.md) exists because nobody trusted on its
own, so a pip scan is noisier than an npm scan by construction and no amount of
parsing care changes it.

[False positives](../detection/false-positives.md) has this costing a real one on
a real fixture.

The upgrade is a different file, not a better reader, and both of those files
read today. [`poetry.lock` and `uv.lock`](poetry-uv.md) record the resolved graph:

```console
$ ./target/release/stranger scan fixtures/poetry-m.poetry.lock

  poetry-m.poetry.lock     233 packages   (75 direct · 158 transitive)

  no findings
  risk 0/100    9ms    third-party deps used to compute this: 0
```

158 of those 233 are transitive, and they can only be called transitive because
the edges are in the file. That is the number `requirements.txt` cannot produce —
it reports every package as direct — and clause 3 is the rule that spends it.

## Which rules can fire

| rule | on pip |
|---|---|
| slopsquat | yes, with clause 3 doing nothing |
| pinning | yes, and only here |
| install-script | no — the format records nothing equivalent |
| drift | technically, but a well-formed file cannot trigger it |
| trivial | technically, but the name list is npm micro-packages |

## What it parses

PEP 508 requirements, which are more than a name and a version:

```text
flask [async] >= 3.0 ; python_version < "3.10"
requests==2.31.0 \
    --hash=sha256:aaa \
    --hash=sha256:bbb
pkg @ https://host/pkg.whl
```

The order the pieces come off matters and is the reason the reader is longer than
it looks.

**Continuations join first**, before anything else sees the text, which is also
pip's order. The line number reported for a joined line is where the logical line
*started*, so an editor gets sent to the right place.

**Comments are `(^|\s+)#.*$`**, not "cut at the first `#`". The difference is
load-bearing: `pkg @ https://host/x.zip#sha256=…` puts a `#` in a URL fragment,
and cutting at the first one truncates the URL into something that still parses.

**Per-requirement options split by token**, not by line, because continuations
have already been joined. That runs before the marker is cut off — pip puts the
options after the marker, and cutting at `;` first would throw the hashes away.

**The environment marker comes off before the version is read.** A marker is
expression syntax carrying the same operators a specifier does — `; python_version
< "3.10"` has a `<` in it — so a classifier that ran first would read the marker as
a range and report a pinned requirement as unpinned. Order, not cleverness.

**Extras come off after the name and before the specifier.**
`flask[async]>=3.0` puts the bracket group between the two. Which extras were
asked for is not recorded: an extra changes what gets installed alongside, never
which version of this name gets installed, and the version is what this reader is
for.

## What it skips

**`-r base.txt` and `-c constraints.txt` are not followed.** An include means file
IO, relative-path resolution and cycle detection, and it quietly turns one file's
audit into a directory crawl. Point `stranger` at the other file.

**`-e` editables are not packages.**

**`--index-url` and `--extra-index-url` are dropped**, and that omission is worth
naming rather than hiding. An extra index is the dependency-confusion vector, and
a line adding one is more interesting than most of the packages under it. It is
dropped because there is nowhere honest to put it: `Tree` holds packages, and this
is a fact about the file. Minting a `Package` to carry it would put a lie in the
package count and a fake name in the report. The upgrade is a field on `Tree` and
a rule that reads it.

## What the format does not record

- **A dependency graph.** Covered above; it is the big one.
- **Install-time code execution.** A source distribution can run whatever
  `setup.py` wants during installation, and this file does not say that it will.
- **A dev/test split.** That lives in a second file whose name is not something to
  guess from.
- **Optionality.** An environment marker is a condition, not npm's
  failure-is-tolerated `optional`, so `optional` is always false.
- **Integrity, usually.** `--hash=sha256:…` is the only integrity this format has
  and it is opt-in. Presence is recorded; nothing verifies it.

## Names are kept as written

`Pillow`, `python-dateutil` and `python_dateutil` are one project to PyPI under
PEP 503, and three different strings in a file. Folding them at read time would
have the report quote a name that is not in the file, so the reader keeps the
spelling and the corpus normalises at comparison time instead:

```console
$ ./target/release/stranger scan --format json fixtures/poisoned.requirements.txt | jq -r '.findings[0].package'
python-dateutils
```

## Syntax errors carry a position

```console
$ printf 'flask[async>=3.0\n' > /tmp/bad/requirements.txt
$ ./target/release/stranger scan /tmp/bad/requirements.txt
stranger: `flask[async>=3.0` has an unclosed `[` in its extras at 1:1
```

Line and column are 1-based, counted on the logical line. For a joined line the
column points at the start of the first piece, and the message quotes the
fragment so the rest is findable.

```console
$ ./target/release/stranger scan -v fixtures/reqs-xs.requirements.txt
```
