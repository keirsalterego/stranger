# DECISIONS.md

Why the things that are the way they are. Written as they were decided, not
reconstructed afterwards.

The second half is a written defence of the design, because the rules say judges
follow up in writing and that an artifact which cannot be defended in writing
scores accordingly.

---

## One crate, not a workspace

`stranger` is twenty-five source files in one crate. It could be `stranger-json`,
`stranger-toml`, `stranger-lock` and so on, and that would look more serious.

A workspace here would be an abstraction with one consumer. None of these modules
are separately useful, none are separately versioned, and splitting them buys
nothing except a longer `cargo build` and a `Cargo.lock` with more `[[package]]`
blocks in it — which, for an entry whose central claim is that the file contains
exactly one, is an actively bad trade.

Modules give the same separation. `cargo build` at the crate root already tells me
if `json.rs` broke `npm.rs`.

## Skipping the Single File bonus, on purpose

There is a +5 bonus for shipping as one file. Twenty-five files crushed into one
`main.rs` trades a 25% criterion for a 5% bonus, and the 25% one is Code Quality
judged by a Rust reviewer who will not enjoy scrolling past a JSON parser to reach
an argument parser.

Stating the trade-off rather than quietly ignoring the bonus, because a declined
bonus with a reason reads as judgement and a declined bonus with no reason reads
as an oversight.

## The corpus is data, and it is compiled in

160,066 package names ship inside the binary via `include_str!`. The three lists
in `corpus/` are about 3 MB of text, and they are the bulk of the release binary —
`wc -c corpus/*.txt` against `ls -l target/release/stranger` is the check.

The alternative — fetching at runtime, or reading a cache directory — would have
made the tool's central claim false. `stranger` works on a plane. There is no
"corpus not found" failure mode, no first-run download, no stale-cache logic, and
no code path where a network timeout changes the answer. Three megabytes is a
cheap price for deleting an entire category of failure.

Rust's standard library has no TLS, so a network request is not merely forbidden
by policy here, it is unavailable. Worth saying out loud as a design property
rather than apologising for it as a limitation.

## Why the in-degree clause exists

Edit distance is not a rule. `lodash.assign` is two edits from `lodash.assignin`,
`object-assign` is one from `object.assign`, and npm contains thousands of such
pairs where both names are real — all four of those are in `corpus/npm.txt`. A
threshold loose enough to catch a typo catches legitimate siblings, and precision
collapses.

The observation that separates them is not about spelling. A hallucinated package
is always a root dependency, because nothing real has ever heard of it. The only
reference to `lodahs` anywhere in the world is the manifest being audited. Real
packages, including the boring near-miss siblings, are depended upon by other
packages.

So: not in corpus, **and** within edit distance 2, **and** in-degree zero.

### The refinement the fixtures forced

The first version counted every dependency edge as evidence. Both monorepo
fixtures then reported *zero* direct dependencies for projects with 582 and 1,390
packages, because both declare `workspaces` and keep almost nothing in the root
manifest.

Fixing that by "also read the workspace members" would have been wrong in an
interesting way. An edge out of a workspace member is the same manifest, by the
same author, as the root — if a model wrote `apps/desktop/package.json`, a
hallucinated name in it arrives with an in-edge and clause 3 never looks at it.

Workspace-member edges are recorded as roots, not as evidence. Same author, same
lack of independent confirmation.

## Measuring the idea instead of asserting it

Against the full corpus the clause is worth exactly nothing: 1.000 precision and
1.000 recall with it and without it. That is in the README, at the top of the
table, because a result that undercuts my own idea is the one most worth
publishing.

It measures nothing because the corpus contains every package in every fixture, so
clause 1 alone suffices and nothing else can show a difference. That is not a
property any real corpus has. So the experiment thins the corpus and re-measures:
at 90% coverage the clause takes false positives from 95 to 3, at no cost to
recall.

Making the corpus a parameter of the rule (`slopsquat::Config::corpus`) rather than
a global is what made that measurable. An assumption you cannot vary is one you
cannot measure.

## The TOML subset

One parser reads `Cargo.lock`, `poetry.lock` and `uv.lock`. Three formats for the
price of one — two ecosystems, since poetry and uv are both PyPI — which bought more per line written than anything after JSON.

**Accepted:** `key = value` at top level and inside tables; `[table]` and
`[dotted.table]` headers; `[[array.of.tables]]`, which is `[[package]]` and the
whole reason the module exists; basic strings with `\b \t \n \f \r \" \\ \uXXXX
\UXXXXXXXX`; literal strings; multi-line `"""…"""` and `'''…'''` including the
line-ending backslash fold; decimal integers with `_` separators and an optional
sign; `true`/`false`; arrays over any number of lines with a trailing comma
allowed; single-line inline tables; `#` comments.

**Refused, each with a line and column:** floats, dates, times and date-times as
bare values; hex, octal and binary integers; dotted keys (`a.b = 1`) outside a
table header; inline tables spread over several lines, which is TOML 1.1;
duplicate keys; a `[table]` header that reopens a table already defined; a
header that reaches *past* a sealed value, so `a = {b = 1}` followed by `[a.c]`
is refused and not only `[a]`; a header path nested deeper than 64; and a bare
carriage return.

The last three are all from the final day and each is worth a line.

**A deep header used to abort the process.** `descend` is a loop, so nothing in
the parser recursed and the depth limit was never applied to headers. But the
nested `Value::Table` chain it *builds* is freed recursively, one stack frame per
segment. `parse` returned `Ok` on a 200,000-segment header in 333 ms and then the
program died in `Drop` — a stack overflow, which `panic = "abort"` makes
unrecoverable, taking every sibling lockfile's findings with it. The threshold on
a worker thread's 2 MiB stack was about 30,000 segments, or 70 KB of input.

**A bare carriage return was silently dropping a key.** Neither `skip_blank` nor
`end_of_line` stopped at `\r`, so `# c\rname = "lodash"\nversion = "1"\n` parsed
`Ok` as `{"version": "1"}` — the `name` key gone, no error. A lockfile reader that
loses a package without saying so is the exact failure this tool exists to catch.
TOML 1.0 settles it twice: the prose says a newline is LF or CRLF, and the ABNF's
comment body admits no `%x0D`. So it is refused, positioned on the byte, and the
message says "a bare carriage return is not a newline" rather than "expected a
newline" — the character is invisible in an editor and the second message leaves
you staring at a line that looks fine.

**And one thing stopped being refused.** `"a.b" = 1` followed by `[a.b]` used to
report ``table `a.b` is defined twice``. Those are distinct namespaces in TOML
1.0 — a quoted key containing a dot is one key, not a path — and the parser could
not tell them apart because it joined path segments into a flat `String`. A
refused lockfile is a whole dependency tree left unaudited, and poetry writes
`"jaraco.classes" = "*"`, so this was reachable. Canonical keys are a
`Vec<Seg>` now, which makes the collision impossible by construction rather than
escaped around.

Refusing beats guessing. A parser that improvises at a construct it does not know
produces a plausible wrong answer, and a plausible wrong answer in a security
tool is worse than an error — an error you investigate, a wrong answer you act on.

Three things the fixtures taught that guessing would have missed:

- **The subset is only sufficient because `uv.lock` writes timestamps as
  strings** — `upload-time = "2026-03-26T01:21:00.379Z"`. Had it used TOML
  datetimes, this parser would refuse the file rather than misread it. The only
  bare integers across all six fixtures are `version` and `revision`: 1, 3, 4.
- **poetry writes quoted keys containing dots**: `"jaraco.classes" = "*"`. That
  is one key whose name contains a dot, not a dotted key. Quoting is what
  decides, not the dot — and conflating them did not invent a `jaraco` table, it
  refused the file, which is worse in a different direction. See above.
- **No triple-quoted string appears anywhere in the corpus.** Not one, across six
  real lockfiles, contrary to what I assumed going in — I had expected poetry to
  use them for descriptions. They are implemented anyway, because a lockfile is
  permitted to contain one and mis-reading it would be worse than refusing it,
  but nothing in the corpus exercises that path.

## The YAML subset

The same reasoning governs `src/yaml.rs`, and the stakes there turned out to be
higher than "a file gets refused".

**Accepted:** block mappings and sequences; single- and double-quoted scalars;
plain scalars; flow mappings and flow sequences, both inline after their key and
on the line below it; comments. **Refused with a position:** anchors, aliases and
tags anywhere, including inside a flow key; tabs used for indentation; a flow
indicator opening a plain scalar or key; an empty mapping key, in either context;
a flow mapping spanning lines.

Two of those are from the final day, and the first is the worst bug in the
repository's history.

**A legal reformat of a lockfile turned its findings off, with no error.** These
two spellings are the same YAML:

```yaml
resolution: {integrity: sha512-AA}
```
```yaml
resolution:
  {integrity: sha512-AA}
```

`block()` only ever dispatched to `mapping` or `sequence`, so the second form
went to the plain-key scanner, which stopped at the first `: ` and returned the
key `{integrity`. That turned `has_integrity` false and moved the package from
`Origin::Registry` to `Origin::Elsewhere` — and the slopsquat rule skips every
`Elsewhere` package. Same file, same packages, three `HALLUCINATION RISK`
findings one way and **none** the other, silently. Detection evasion in a
detection tool, reachable by running the file through any YAML formatter.

The fix is both halves: `block()` dispatches a leading `{` or `[` to the flow
parsers, and the plain scalar and key scanners refuse a flow indicator instead of
swallowing it. Either alone leaves the other spelling open. The proof is the
whole 254 KB fixture with all 1,698 of its flow collections moved onto their own
line: same 850 packages, same 1,851 edges, same roots, same keys.

**Also: the flow scanner was O(n²), and so was its error path.** `line()` walks to
the newline and was called once per flow item, which then broke a few bytes in.
500,000 items took 471.75 s; they take 139.6 ms now. A hostile file did not have
to be *valid* to stall the auditor, which is worth saying out loud — an unclosed
1 MB flow collection cost the same 471 s.

**Billion-laughs is closed**, and not by a limit: there is no alias resolution
machinery in this parser at all, so there is nothing for an expansion to expand.

Then the decision the module is really about. YAML 1.1
implicit typing turns `no`, `on`, `off` and `y` into booleans — and all four are
registered npm package names. A reader that turned the key `no@1.0.0` into a
boolean would drop a package out of an audit without a word, so exactly two tokens are
typed (lowercase `true` and `false`, which `pnpm-lock.yaml` needs) and everything
else stays a string.

Checkable against this repository only for half of them: `no` and `on` are in `corpus/npm.txt`; `y` and `off` are registered on npm and are not, because the corpus is the top 140,066 names by download count and not the whole registry. Which is the same distinction the tool makes about `Origin::Elsewhere`: absence from a popularity sample is not evidence a package does not exist.

## What the xorshift is for

Rust's standard library has no random number generator, and two things here need
one: property tests over random short strings, and deterministic corpus thinning
for the ablation.

Five lines of xorshift64\*, seeded from `SystemTime` nanoseconds in the property
tests (and printed, so a failure replays) and from a fixed constant in the
ablation (so the published table reproduces).

It has a short period and fails statistical tests a real generator passes. That is
fine for generating four-letter strings out of a four-character alphabet. It would
not be fine for anything security-sensitive, and nothing here is: `stranger`
computes no hashes, verifies no signatures, holds no key material, and makes no
nonces. The rules forbid rolling your own crypto and this is not crypto.

## Reading files that other tools produced

The rules forbid shelling out to an installed tool, and the FAQ rules this design
in explicitly: *"Parsing files those tools already produced is fine, because
nothing third-party ends up in your artifact."*

Two conditions attach and both are honoured. Disclosed, in `STDLIB.md` and
`corpus/PROVENANCE.md`. And degrades gracefully: a directory with no lockfile
prints what was looked for and exits 0, rather than being useless or panicking.

`stranger` never executes another program. Not `npm`, not `pip`, not `cargo`, not
`git`. There is no `std::process::Command` anywhere in `src/`.

---

# Defence

## Sanitising at the read seam, not at the print sites

A lockfile is a file written by strangers. That is the premise of the whole tool,
and for two days it was true of everything except the renderer, which printed
package names and versions to a terminal unfiltered. A version string of
`1.0.0\x1b[2K\x1b[1A\x1b[2K` scrolls the two lines above it out of existence — the
`HALLUCINATION RISK` heading and the finding's own name — while the process still
exits 1. `\x1b[2J` clears the screen. It is the one bug in this repository a
*malicious* input could exploit rather than merely trip over, and a tool whose
findings can be deleted by the file it is auditing is not an auditing tool.

The obvious fix is a `sanitize()` call at each print site. It is also the wrong
one. `report.rs`, `tree.rs` and five rules' `detail` strings all format package
names, which makes "sanitise before printing" a rule with a dozen call sites and
one of them missed by whoever adds the thirteenth. So it happens once, in
`lock::read`, over every string a reader took out of the file. A name that reaches
the rest of the program is a name that has been through it, and a seventh reader
gets it for free.

Two consequences worth defending:

**Replaced with U+FFFD, not dropped.** The cell still takes a column, so the
reader can see something was there rather than seeing a name that silently got
shorter. It also keeps `term::width` honest — the escape bytes were being counted
as display columns, so a hostile version string knocked every row after it out of
alignment as well.

**The JSON writer gets the scrubbed string too, although it never needed it.** It
escapes correctly, so `\u001b` would have been perfectly safe there. But two
output surfaces disagreeing about what a package is called is worse than either
answer on its own, and a consumer diffing the human report against the JSON should
not find a difference the tool invented. Nothing real is lost: no registry permits
a control character in a name — npm allows URL-safe characters, PyPI normalises to
`[a-z0-9.-]`, crates.io to `[A-Za-z0-9_-]` — and no version scheme has one either.

`fixtures/hostile.package-lock.json` is the file that does all of this, and it is
shipped rather than described. It is legal JSON throughout, with every escape in
`\uXXXX` form, so it tests the renderer and not the parser.

## Why the JSON object carries no elapsed time

The human report prints `41ms`, because that is half the pitch and nobody diffs a
terminal. The machine-readable object does not, and that is a deliberate
subtraction rather than an omission.

This file offers `diff <(stranger scan a --format json) <(stranger scan b --format
json)` as the reason `stranger diff` was cut — you do not need a subcommand for
something two shell redirections already do. That recipe printed a difference on
every single run, because `elapsed_ms` was the one field that changed between two
scans of the same tree. Everything else was already byte-identical, including the
order of the findings and the order of the files in a directory scan, both of
which cost real design effort to guarantee.

So the field went. A promise that a scan is reproducible is worth more than a
measurement a consumer can take with `time`, and shipping the recipe while
shipping the one field that breaks it was the kind of thing a judge finds in
thirty seconds. `tests/cli.rs::json_is_byte_identical_between_runs` holds it, and
also holds that the human report kept its timing.

## Walk me through how your JSON parser handles a lone high surrogate.

It rejects it, with position.

JSON strings are sequences of UTF-16 code units, so anything outside the Basic
Multilingual Plane arrives as a surrogate pair — 🦀 is `\uD83E\uDD80` — and has
to be recombined into one scalar. `unicode_escape` reads four hex digits, and if the result is in
`0xD800..=0xDBFF` it requires the next two bytes to be `\u` and the following four
digits to be a low surrogate in `0xDC00..=0xDFFF`, then combines them:
`0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)`.

A high surrogate followed by nothing, or by a non-surrogate, or a low surrogate
standing alone, is `Error::Syntax`. The position is where the parser *discovered*
the problem, which is one past the escape rather than at its backslash: on
`{"a":"\uD800x"}` it says `high surrogate not followed by a low surrogate at
1:13`, and the `\u` starts at column 7. That is the honest description of what the
column means — this file said "the line and column of the escape" for most of the
weekend, which is six columns off. Pointing at the backslash would be friendlier
and would mean carrying the escape's start through `unicode_escape`; it is a
papercut, not a defect, and it is written down here rather than rounded off.

The alternative was substituting U+FFFD, which is what a lot of parsers do. That
would have been wrong here specifically: this parser's output feeds package-name
comparison, and silently rewriting a byte of a package name is precisely the class
of bug the tool exists to find. A corrupt name should stop the scan, not quietly
become a different name.

`tests/json.rs::lone_surrogates_are_rejected` covers all three cases.

## Why Damerau and not plain Levenshtein? Show me a case where it matters.

`lodahs` against `lodash`. It is a transposition — the `h` and `s` swapped —
which is Damerau distance 1 and Levenshtein distance 2.

**The answer this file gave until the last day was wrong, and wrong in the
flattering direction.** It said that at distance 2 plain Levenshtein "also matches
`logash`, `nodash`, `lodas`, `loash` and a long tail of real registry entries", so
the threshold that catches the typo drags in everything else. Every clause of that
is false. None of those four names is in `corpus/npm.txt`, so none of them is a
real registry entry the rule could drag in. All four are at distance **1** from
`lodash`, not 2. And `lodash` has exactly the same neighbour count under both
metrics at every threshold that matters — 1, 6 and 49 at k = 1, 2 and 3.

The arithmetic makes the old claim impossible, which is the part worth admitting.
Levenshtein is pointwise **greater than or equal to** Damerau, because Damerau
permits every edit Levenshtein does plus transposition. So Damerau-at-k is always
the *more permissive* of the two. It can never be the tighter filter, and an
argument that it is has the inequality backwards. At the shipped threshold of 2,
plain Levenshtein returns **1** candidate for `lodahs` and Damerau returns **3**
(`lodash`, `loadjs`, `loodash`).

**So here is the case where it actually matters.** At k = 1 the two metrics
disagree completely: Damerau finds `lodash`, Levenshtein finds nothing at all.
Everything Damerau buys is on that side of the line —

- **The reported distance.** The finding says `d=1 from "lodash"` rather than
  `d=2`, which is the difference between "somebody fat-fingered lodash" and "this
  is two edits from something, like a hundred other names are".
- **The tie-break.** `nearest_in` sorts on `(distance, name length)`, so a metric
  that scores the transposition 1 puts the real parent first instead of leaving it
  tied with whatever else is at 2.
- **The threshold you could drop to.** If the crates.io corpus were as complete as
  the npm one — the real limit, measured in the ablation — k = 1 becomes arguable,
  and at k = 1 Damerau is the only one of the two that still catches a
  transposition. That is the upgrade path, and it is why the metric is the one
  that is there.

What it does **not** buy is extra detections at the threshold that ships. Measured
across every name the rule fires on in every fixture, at k = 2 the two metrics
fire on **exactly the same set**. `tests/distance.rs::
damerau_changes_the_distance_not_which_names_fire` holds all of it, so the claim
cannot rot back to the comfortable version.

Transposition is the most common single-character slip a human or a model
produces. Scoring it 1 is worth having. Claiming it changes what gets caught at
k = 2 was worth checking, and it did not survive the check.

There is a second decision inside this one. The implementation is *unrestricted*
Damerau-Levenshtein (Lowrance-Wagner), not the optimal string alignment variant
that almost every crate — including `strsim` — ships under the name. OSA refuses to
edit inside a span it has already transposed, so it scores `CA` against `ABC` as 3
when the true distance is 2, and it is not a metric: it fails the triangle
inequality.

Nothing here needs the triangle inequality today. It is implemented correctly
because a distance function that quietly is not a metric is fine right up until
somebody indexes with it, and the honest version cost about fifteen lines.
`tests/distance.rs` proves identity, symmetry and the triangle inequality over
20,000 random triples, and `unrestricted_not_osa` pins the `CA`/`ABC` case.

## Which clause carries the most signal, and how do you know?

With a complete corpus, clause 1. The ablation is unambiguous: at 100% coverage the
in-degree clause changes nothing, because a corpus containing every package in
every fixture already answers the question by itself.

With a realistic corpus, clause 3, by a wide margin. At 90% coverage it is the
difference between 95 false positives and 3. At 70%, between 332 and 16. It never
costs a true positive at any coverage level, which is asserted in the test rather
than observed once.

I know because `make ablation` runs it, not because it seemed likely.

The honest framing is that clause 3 is not what finds hallucinations — clause 1
does that. Clause 3 is what stops the rule from disintegrating as clause 1 decays,
and clause 1 decays continuously, because npm accepts thousands of new names a day
and every corpus is a snapshot.

## Where does this tool give a wrong answer, and what would you do about it with a week?

Three places.

**A package published after the corpus snapshot** looks exactly like a package that
does not exist. If it also happens to be within two edits of a popular name and
nothing depends on it yet — which describes a lot of genuinely new packages — it is
a false positive. This is the failure mode the ablation table is a measurement of.
With a week: ship the corpus with a timestamp, and treat "resolved from the
registry with an integrity hash recorded" as weak evidence of existence, since a
name that was resolvable at lockfile-generation time did exist then.

**A private or internal registry.** Every first-party package published to an
internal registry is absent from a public corpus by definition. Right now the only
protection is the `link: true` / workspace-member exclusion, which catches
monorepo members and not a company's internal published packages. With a week:
read `.npmrc` scope-to-registry mappings and exclude any scope that resolves
somewhere other than the public registry.

**Flat formats.** `requirements.txt` has no dependency graph, so clause 3 is
vacuous and the rule runs on two clauses. It is measurably weaker there and the
README says so. With a week: read `poetry.lock` and `uv.lock` instead where they
exist, since both do record the graph.

## Why zero unsafe — was that hard, or free?

Free. It would be overclaiming to say otherwise.

`stranger` parses text and walks graphs. There is no FFI, no custom allocator, no
lock-free data structure, no place where the borrow checker was standing between
me and a working program. The crate root says `forbid(unsafe_code)` rather than
`deny` because zero was achievable without effort, and at zero cost the stricter
attribute is simply the correct one.

The one place it could plausibly have appeared is the terminal detection, where
the traditional answer is an `unsafe` FFI call to `libc::isatty`.
`std::io::IsTerminal` has been stable since 1.70 and does it in safe code.

## You read `integrity` fields but never check them. Why?

Because Rust's standard library has no cryptography at all — no SHA-2, no SHA-1,
nothing. Verifying a `sha512-...` integrity field means implementing SHA-512, and
the rules explicitly rule out rolling your own crypto.

They are right to. A hand-rolled SHA-512 written in a weekend, used to decide
whether a package tarball has been tampered with, is worse than no check at all,
because it produces a green tick nobody has any reason to trust.

So the tool reports whether the field is present and never whether it is correct,
and the README says that in the LIMITS section rather than leaving it to be
discovered. Reporting presence without verification is the honest half of the job.
Claiming the other half would be the dishonest whole.

This is the constraint biting in public, which is the subject of the entire event.

## Isn't reading npm's lockfile just shelling out to npm with extra steps?

No, and the distinction is the one the FAQ draws: *"Parsing files those tools
already produced is fine, because nothing third-party ends up in your artifact."*

Shelling out means the tool does not work unless something else is installed. The
dependency is real, it is just hidden behind a `Command::new`. `stranger` has the
opposite property — it works best precisely where npm is *not* installed, because
auditing a lockfile you did not write is exactly when you do not want to install
its toolchain first.

The demonstration is a directory containing nothing but a `package-lock.json`, on
a machine with no Node, no npm and no network. It scans in milliseconds.

There is no `std::process::Command` in `src/`. `grep -rn "Command::new" src/`
returns nothing, and that is a one-line check anyone can run.

---

## Cuts

Recorded here as decisions rather than omissions.

The plan had a line through the build list: everything below it was to be cut at
H30 if unstarted, by the clock rather than by feeling. Two of the four items below
that line landed anyway — `yaml.rs` with the pnpm reader, and the parallel scan on
`std::thread::scope`. Two did not: `yarn.lock` and `stranger diff`, both written
up below. One thing above the line was cut outright as well.

**`yarn.lock` — cut, and it was the right call twice over.** It was below the line,
and there is no yarn fixture on this machine, so it would have shipped tested
against a file I wrote myself to match my own reading of the format. Every other
reader here was built against real lockfiles from real projects, and three of them
found something I had assumed wrong. A reader validated only against my
assumptions would have been the one place in this repository where that check did
not happen.

**`stranger diff old.lock new.lock` — cut.** Below the line, and a whole second
verb: two trees, a matching problem between them, and a report format nothing else
uses. `--format json` already emits a stable, ordered object per lockfile, so
`diff <(stranger scan a --format json) <(stranger scan b --format json)` covers the
case without a subcommand. Ordering findings deterministically was the part worth
doing, and that shipped.

**`go.mod` — cut, then uncut, and the reason for the cut turned out to be the
reason to ship.** The original call: the parser is trivial, the corpus is not.
`proxy.golang.org` publishes no ranked list, and a Go module path is a domain, so
"not in the corpus" would mean "not in a list nobody publishes" rather than "does
not exist", which turns the detection rule into noise. Shipping the parser
without the corpus looked like a seventh format on the README and a rule that
silently never fires.

What changed is where the silence lives. A rule that never fires because a list
happens to be empty is the thing worth refusing; a rule that is *switched off for
an ecosystem, in one line, with the reason written next to it* is a different
object. `slopsquat::scan` now returns on its first line when the ecosystem's
corpus is empty — checked against the compiled-in list rather than the one the
ablation passes in, so no configuration can turn it back on — and `tests/gomod.rs`
hands it a one-edit neighbour of a real module to prove it. `corpus::names` still
returns an empty slice for `Ecosystem::Go` and `tests/corpus.rs` still asserts
that emptiness.

So the reader ships and the rule does not, both on purpose: 174 requirements out
of `gomod-m`, split 50 direct against 124 `// indirect`, no findings, and the
README says in the Limits section that no findings is what a Go scan produces.
`go.sum` stays cut, and `src/lock/gomod.rs` argues that one out at length — it
holds hashes for versions that lost the selection, so counting from it overstates
the tree, and the standard library has no SHA-256 to check one with anyway.

**The Single File bonus — cut deliberately, and it was never close.** Twenty-five
files crushed into one `main.rs` trades a 25% criterion for a 5% bonus.

**`src/semver.rs` — written for a range matcher that got cut, and then earned its
place anyway.** Ten tests, including the prerelease precedence rules from section
11 that most implementations get wrong. It spent a day as genuinely dead code, and
three files here and in the book said so. That stopped being true in `58c101f`:
the drift rule has to put one package's several versions in *an* order before it
prints them, and byte order is not that order — it puts `10.2.5` first and
`1.0.0-rc.1` above `1.0.0`. So `drift.rs` imports `Version` and sorts with it.
Equality is still all the rule *asks*; ordering is what the reader needs.

Worth recording as a decision rather than quietly corrected, because the sentence
survived in three files for a day after it went false, and the thing that finally
caught it was a checker that runs the book's own console blocks.

**Not cut, and it should have been on the list from the start: the demo video.**
It is the one deliverable with no partial credit.
