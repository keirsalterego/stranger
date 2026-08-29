# The co-occurrence rule

Edit distance on its own is not a rule.

`http-proxy-agent` and `https-proxy-agent` are both real npm packages, both
depended on by other packages in `npm-xl`, and one edit apart. So are
`safe-buffer` and `safer-buffer`. Take just the 1,077 distinct names the npm
fixtures install: between them they have 9,453 neighbours within distance 2 in a
140,066-name corpus, every one a package that exists. Any threshold loose enough
to catch a typo is loose enough to catch a legitimate sibling. Precision collapses
and the tool becomes noise.

The clause that separates them is not about spelling at all:

> A hallucinated package is a **root** dependency. Nothing depends on it,
> because nothing real has ever heard of it. A model put it in your manifest; no
> maintainer ever put it in theirs.

`https-proxy-agent` is depended on by other packages. `lodahs` cannot be, because
it does not exist — the only reference to it anywhere in the world is the manifest
under audit.

## Three clauses

A name is reported when all three hold.

1. It is **not in the corpus** of names known to exist.
2. It is **within edit distance 2** of a name that is.
3. **Nothing in the lockfile depends on it.** Its in-degree is zero.

Clause 1 is a binary search over a sorted list compiled into the binary. Clause 2
is Damerau-Levenshtein, the unrestricted Lowrance-Wagner variant rather than the
optimal-string-alignment version most libraries ship under that name, with the
threshold at 2. Clause 3 is a lookup in a vector of in-degrees built from the
lockfile's own edges.

They run in the order 1, 3, 2. Clause 1 first because it is a binary search and
it eliminates all but a couple of dozen names. Clause 3 before clause 2 because
it is one array index and clause 2 is a linear scan of 140,066 names — on the
`npm-xl` fixture that ordering is most of the difference between 413 ms and
something considerably worse.

Packages marked first-party are skipped before any of it. Somebody in this
repository wrote them; they are not strangers.

Every hit is `critical`, and it is the only rule that is. It is also the only one
whose findings are always listed rather than collapsed to a count.

## Clause 3, demonstrated

Two lockfiles, the same fake package, one edge of difference. Write the first:

```console
$ mkdir -p /tmp/a && cat > /tmp/a/package-lock.json <<'EOF'
{
  "name": "demo",
  "lockfileVersion": 3,
  "packages": {
    "": { "dependencies": { "expres": "^4.18.2" } },
    "node_modules/expres": {
      "version": "4.18.2",
      "resolved": "https://registry.npmjs.org/expres/-/expres-4.18.2.tgz"
    }
  }
}
EOF
$ ./target/release/stranger scan /tmp/a

  package-lock.json        1 packages   (1 direct · 0 transitive)

  ⚠  HALLUCINATION RISK     1
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent

  risk 77/100    48ms    third-party deps used to compute this: 0
```

Now the same name, reached through a real package that claims to depend on it:

```console
$ mkdir -p /tmp/b && cat > /tmp/b/package-lock.json <<'EOF'
{
  "name": "demo",
  "lockfileVersion": 3,
  "packages": {
    "": { "dependencies": { "body-parser": "^1.20.2" } },
    "node_modules/body-parser": {
      "version": "1.20.2",
      "dependencies": { "expres": "^4.18.2" }
    },
    "node_modules/expres": {
      "version": "4.18.2",
      "resolved": "https://registry.npmjs.org/expres/-/expres-4.18.2.tgz"
    }
  }
}
EOF
$ ./target/release/stranger scan /tmp/b

  package-lock.json        2 packages   (1 direct · 1 transitive)

  no findings
  risk 0/100    7ms    third-party deps used to compute this: 0
```

Clauses 1 and 2 are identical in both files. `expres` is absent from the corpus
either way and one deletion from `express` either way. The only thing that
changed is that a maintainer other than you wrote the name down, and the rule
goes quiet.

That is the conservative direction on purpose. An extra in-edge suppresses a
finding; it can never invent one. The same reasoning is why `peerDependencies`
counts as an edge alongside `dependencies`, `devDependencies` and
`optionalDependencies` — a peer dep is still a real maintainer writing down a
real name, and counting it can only make the rule quieter.

## Which edges count

Not all of them. An edge is evidence only if a *stranger* drew it.

Edges out of the root manifest do not count. That manifest is the thing under
audit; "the file an LLM helped write lists this package" is not evidence that the
package exists. Those go into a separate `roots` list, not into `edges`.

Neither do edges out of a workspace member. Same repository, same author, same
absence of evidence. That refinement is doing real work: both monorepo fixtures
here declare `workspaces` and keep almost nothing in the root manifest, so a
reader that only looked at the root entry would report **zero** direct
dependencies for a 576-package project. A hallucinated name added to
`apps/desktop/package.json` would arrive with an in-edge and never be examined.

```console
$ mkdir -p /tmp/c && cat > /tmp/c/package-lock.json <<'EOF'
{
  "name": "monorepo",
  "lockfileVersion": 3,
  "packages": {
    "": { "workspaces": ["apps/*"] },
    "apps/desktop": { "dependencies": { "expres": "^4.18.2" } },
    "node_modules/desktop": { "resolved": "apps/desktop", "link": true },
    "node_modules/expres": {
      "version": "4.18.2",
      "resolved": "https://registry.npmjs.org/expres/-/expres-4.18.2.tgz"
    }
  }
}
EOF
$ ./target/release/stranger scan /tmp/c

  package-lock.json        1 packages   (1 direct · 0 transitive · 2 workspace)

  ⚠  HALLUCINATION RISK     1
     expres@4.18.2            not in corpus · d=1 from "express" · root-only, no parent

  risk 77/100    45ms    third-party deps used to compute this: 0
```

The workspace member declared it and the rule still fires. Compare with `/tmp/b`,
where a third-party package declared it and the rule did not.

## Why distance 2

At 2, `lodash` is within range of 6 corpus names. At 3 it is within range of 49 —
`lodash-es`, `lodash.eq`, `lodash.gt`, `lodash.lt`, `ldap`, `slash`, `soda` and
forty-two more, every one of them a real package — and precision on the fixtures
falls off a cliff. Two still catches every single-character slip — deletion,
insertion, substitution, transposition — which is the entire population of typos
a model actually produces.

Transposition is why the distance function is Damerau and not plain Levenshtein.
`lodahs` is one transposition from `lodash`: Damerau distance 1, Levenshtein
distance 2. Under Levenshtein it would sit in the same bucket as a large
population of legitimate siblings, and the threshold that catches it catches half
the registry with it.

When several corpus names are within range, ties go to the shorter one. `asn1s` is
one edit from both `asn1` and `asn1js`, and the finding names `asn1`. That is a
display choice and not a detection one: the finding fires on either neighbour, and
which of them gets printed changes nothing about whether it fires.

## Distance 2 is not a small net

`requests-http` was planted in `fixtures/poisoned.requirements.txt` as a name the
rule was *not* expected to catch — a hallucinated name that is not a near-miss of
anything real. It fires anyway:

```console
$ ./target/release/stranger scan fixtures/poisoned.requirements.txt

  poisoned.requirements.txt 6 packages   (6 direct · 0 transitive)

  ⚠  HALLUCINATION RISK     2
     python-dateutils@2.9.0   not in corpus · d=1 from "python-dateutil" · root-only, no parent
     requests-http@1.0.2      not in corpus · d=2 from "requests-html" · root-only, no parent

  ⚠  UNPINNED               3     no exact version recorded

  risk 79/100    15ms    third-party deps used to compute this: 0
```

`requests-html` is a real PyPI package two edits away. The finding is a true
positive — `requests-http` does not exist — but the prediction about it was
wrong, and it was wrong because nobody checked whether a real neighbour happened
to exist. Two edits reaches further than intuition suggests.

## Where clause 3 is vacuous

On `requirements.txt` there are no dependency edges, so every package trivially
has in-degree 0, clause 3 eliminates nothing, and the rule degenerates to the two
clauses the ablation was written precisely because nobody trusted on their own.
[pip](../formats/pip.md) covers the consequence and
[False positives](false-positives.md) shows it costing a real one.
`stranger tree` refuses to print an in-degree on one of those files at all —
[Looking at one package](../using/tree.md#flat-formats-have-no-graph).

## Is clause 3 worth anything

Against the full corpus, measurably nothing. Against a corpus missing 10% of its
names, it cuts false positives from 95 to 3 at no cost in recall. That is a
number, not an assertion, and it is on the next page.

```console
$ ./target/release/stranger scan fixtures/poisoned.package-lock.json
```
