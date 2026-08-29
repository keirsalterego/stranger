//! Damerau-Levenshtein edit distance.
//!
//! This is the *unrestricted* variant (Lowrance-Wagner), not the optimal
//! string alignment version that most crates ship under the same name. The
//! difference shows up when transposed characters have edits between them:
//! OSA scores `CA` against `ABC` as 3 because it refuses to edit a substring
//! it has already transposed, and the true distance is 2. That also makes OSA
//! not a metric — it fails the triangle inequality — so the property test at
//! the bottom of `tests/distance.rs` would not pass against it.
//!
//! Nothing in `stranger` currently needs the triangle inequality. It is here
//! because a distance function that quietly is not a metric is the kind of
//! thing that is fine until someone indexes with it, and the honest version
//! cost about fifteen extra lines.

use std::collections::BTreeMap;

/// The ceiling on how far apart two names can be and still be typos of each
/// other. What a *particular* name is allowed is [`budget_for`], which is
/// this or less.
///
/// Two is the smallest ceiling that still finds every planted name in the
/// fixtures. At one, `requests-http` goes quiet, and it is a true positive:
/// two edits from the real `requests-html`. Three is the direction worth
/// measuring, and the answer is lopsided. On npm, pnpm, poetry, uv and pip it
/// changes nothing whatsoever — the findings at three are identical to the
/// findings at two, fixture for fixture and name for name. On `cargo-l` it
/// goes from zero findings to six. `assert2`, `mavlink`, `petname`,
/// `ros2-client`, `rust-format` and `splitty` are all real crates, and each
/// one lands three edits from something it has no relationship to (`adler2`,
/// `maplit`, `uname`, `oci-client`, `num-format`, `plist`). Four adds five
/// more, spread over `cargo-m`, `npm-xl` and `pnpm-l`.
///
/// That split is the corpus talking, not the threshold. `corpus/npm.txt` has
/// 140,066 names and contains every real package in every npm fixture, so
/// clause one of the slopsquat rule has already eliminated them before any
/// distance is computed and `k` is left with nothing to act on.
/// `corpus/crates-io.txt` has 5,000 — the top of crates.io, not the whole of
/// it — so dozens of real crates survive clause one, and every widening of `k`
/// hands one of them a nearest neighbour. Which makes the threshold the cheap
/// knob and the corpus the real one.
///
/// For shape: at k = 1, 2, 3, 4 the npm corpus holds 1, 6, 49 and 467
/// neighbours of `lodash` and 2, 4, 44 and 145 of `express`, and the PyPI
/// corpus holds 1, 2, 5 and 22 of `requests`. Cost follows — scanning
/// `npm-xl` takes 221ms, 414ms, 630ms, 2,229ms — because the length prefilter
/// and the row-minimum early exit both loosen as `k` grows.
///
/// The honest gap in all of it: no fixture contains a hallucinated name whose
/// nearest real neighbour actually sits at three edits, so this prices a wider
/// threshold without ever valuing it. A fixture with a genuine d=3 typo, or a
/// crates.io corpus as complete as the npm one, is what would tell the two
/// apart. Two also catches every single-character slip — deletion, insertion,
/// substitution, transposition — which is the population of typos a model
/// actually produces.
pub const MAX_EDIT_DISTANCE: usize = 2;

/// How much name it takes to buy one edit of slack.
///
/// [`MAX_EDIT_DISTANCE`] on its own is a claim about pairs of names and says
/// nothing about how long they are, and short names are where that breaks. A
/// three-letter name is within two edits of *something* in every registry —
/// so for a name that short, clause two of the slopsquat rule is not a filter,
/// it is a formality, and the rule collapses to "not in the corpus AND
/// in-degree zero", which is a guaranteed CRITICAL for any real package that
/// happens to sit below the popularity cut.
///
/// Measured, rather than argued. Take each name in a corpus, pretend it is
/// missing — which is what a real package below the cut looks like — and ask
/// whether the rest of the list offers it a neighbour. That is the false
/// positive rate, and it is a function of length:
///
/// | chars | npm k=1 | npm k=2 | pypi k=1 | pypi k=2 | crates k=1 | crates k=2 |
/// |---|---|---|---|---|---|---|
/// | 2 | 99.6% | 100.0% | 88.0% | 100.0% | 36.4% | 100.0% |
/// | 3 | 98.6% | 100.0% | 69.9% | 100.0% | 60.4% | 100.0% |
/// | 4 | 51.9% | 100.0% | 43.9% |  98.9% | 41.4% |  99.1% |
/// | 5 | 40.5% |  97.5% | 34.0% |  93.8% | 18.7% |  78.9% |
/// | 6 | 36.5% |  85.8% | 16.3% |  76.6% | 12.5% |  51.5% |
/// | 8 | 30.0% |  63.0% | 11.3% |  35.6% |  7.0% |  26.6% |
/// | 9 | 27.7% |  55.1% |  5.9% |  23.1% |  4.1% |  19.3% |
/// | 10 | 18.8% |  46.1% |  2.1% |  14.9% |  4.8% |   9.5% |
///
/// Read npm, which is the largest list and therefore the one that offers a
/// neighbour to everything: a hit at k = 1 stops being the likelier outcome
/// at five characters, and a hit at k = 2 stops being the likelier outcome at
/// ten. Below a coin flip is the bar — a clause that fires on most inputs is
/// not evidence about any of them — and five and ten are one edit per five
/// characters, twice. Hence the constant, and hence `len / 5`.
///
/// What the alternatives did, over every fixture, scored against the five
/// planted names (`expres`, `lodahs`, `chalck`, `python-dateutils`,
/// `requests-http`) and counting everything else as a false positive:
///
/// | policy | TP | FP | recall | precision |
/// |---|---|---|---|---|
/// | `2` — what shipped | 7 | 5 | 1.000 | 0.583 |
/// | `min(2, len / 3)` | 7 | 4 | 1.000 | 0.636 |
/// | `min(2, (len - 1) / 3)` | 7 | 3 | 1.000 | 0.700 |
/// | `min(2, len / 4)` | 7 | 3 | 1.000 | 0.700 |
/// | `min(2, len / 5)` | 7 | 1 | 1.000 | **0.875** |
/// | `1` — a tighter ceiling instead | 6 | 4 | 0.857 | 0.600 |
///
/// `(len - 1) / 4`, `(len - 1) / 5` and `len / 6` also score 0.875, and the
/// leave-one-out table is what separates them from `len / 5`: `(len - 1) / 4`
/// hands out two edits at nine characters, where npm still answers 55% of the
/// time, and the other two refuse `nunpy` its edit at five characters, which
/// is a true positive `tests/pip.rs` holds the rule to.
///
/// The four false positives this removes are `ksni` (4 chars, two edits from
/// `jni`), `taze` (4, one from `gaze`), and `bell` and `csi` in the hostile
/// fixture. The one it does not is `tensorflow-gpu`: fourteen characters and
/// one edit from `tensorflow-cpu`, which no length policy can reach and which
/// README LIMITS already owns. A tighter ceiling is not the answer either —
/// the last row is `MAX_EDIT_DISTANCE` at one, and it loses `requests-http`.
///
/// `tests/ablation.rs::edit_budget_policy_sweep` is that table, and prints it.
pub const CHARS_PER_EDIT: usize = 5;

/// Edits allowed against a name of `len` characters.
pub fn budget_for(len: usize) -> usize {
    MAX_EDIT_DISTANCE.min(len / CHARS_PER_EDIT)
}

/// Distance if it is at most `k`, `None` otherwise.
///
/// One question, one answer, three allocations. Anything asking the same
/// question of a whole corpus should build a [`Query`] and keep it.
pub fn within(a: &str, b: &str, k: usize) -> Option<usize> {
    Query::with_budget(a, k).distance_to(b)
}

pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let ceiling = a.chars().count() + b.chars().count();
    Query::with_budget(a, ceiling)
        .distance_to(b)
        .unwrap_or(ceiling)
}

/// One name, expanded once, ready to be asked about a whole corpus.
///
/// `within` re-collected the query into a fresh `Vec<char>` and allocated a
/// fresh DP table for every candidate, and `corpus::nearest_in` puts the same
/// question to up to 140,066 candidates in a row.
///
/// One unknown name against the npm corpus, in CPU milliseconds — wall clock
/// on the box this was measured on is mostly a measurement of the other things
/// running on it — best of five interleaved rounds, twice:
///
/// | | ms |
/// |---|---|
/// | flat scan, fresh buffers per candidate | 37.7 |
/// | flat scan, these buffers | 23.7 |
/// | plus `corpus::ByLength` | 19.7 |
pub struct Query {
    chars: Vec<char>,
    budget: usize,
    /// The candidate, re-expanded in place. Same allocation every time.
    candidate: Vec<char>,
    /// The DP table, resized in place. Same allocation every time.
    table: Vec<usize>,
}

impl Query {
    /// Prepare `name` with the edit budget its length earns — see
    /// [`budget_for`].
    pub fn new(name: &str) -> Query {
        Query::with_budget(name, budget_for(name.chars().count()))
    }

    /// Prepare `name` with a budget the caller chose. `damerau_levenshtein`
    /// wants no ceiling at all, and the sweeps in `tests/` want to vary it.
    pub fn with_budget(name: &str, k: usize) -> Query {
        Query {
            chars: name.chars().collect(),
            budget: k,
            candidate: Vec::new(),
            table: Vec::new(),
        }
    }

    /// Edits allowed against this name. Zero means no candidate can match and
    /// the caller should not start the sweep at all.
    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Characters, not bytes. Length buckets are keyed on this.
    pub fn char_len(&self) -> usize {
        self.chars.len()
    }

    /// Distance to `candidate`, if it is within budget.
    ///
    /// The length check is not just an optimisation: a distance of `k` cannot
    /// change a string's length by more than `k`, so it is exact, and it
    /// rejects whatever a caller's own length filter did not.
    pub fn distance_to(&mut self, candidate: &str) -> Option<usize> {
        // Counted before it is expanded, not after. Every candidate reaches
        // this line and most of them fail it, so the rejected ones must not
        // pay for a `Vec<char>` they will never be compared against.
        if self.chars.len().abs_diff(candidate.chars().count()) > self.budget {
            return None;
        }
        self.candidate.clear();
        self.candidate.extend(candidate.chars());
        bounded(&self.chars, &self.candidate, self.budget, &mut self.table)
    }
}

/// Lowrance-Wagner, with the table offset by one so the algorithm's `-1` row
/// and column have somewhere to live.
///
/// The early exit on row minimum is sound here, which is worth a line because
/// it is not obvious with a transposition rule that reaches back to an
/// arbitrary earlier row. The row minimum rises by at most one per row (any
/// cell is at most its northern neighbour plus one), and the transposition
/// term costs exactly the rows it skips, so it cannot land below the running
/// minimum either.
fn bounded(a: &[char], b: &[char], k: usize, d: &mut Vec<usize>) -> Option<usize> {
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return (m <= k).then_some(m);
    }
    if m == 0 {
        return (n <= k).then_some(n);
    }

    let inf = n + m;
    let w = m + 2;
    // `clear` then `resize` zeroes the table the way `vec![0; _]` did, and
    // keeps whatever capacity the last candidate already paid for. Skipping
    // the zeroing entirely is sound — every cell is written before it is read
    // — and measured 8% on the sweep, which is inside this box's noise floor.
    // Not worth an invariant a reader has to reconstruct.
    d.clear();
    d.resize((n + 2) * w, 0);
    let at = |i: usize, j: usize| i * w + j;

    d[at(0, 0)] = inf;
    for i in 0..=n {
        d[at(i + 1, 0)] = inf;
        d[at(i + 1, 1)] = i;
    }
    for j in 0..=m {
        d[at(0, j + 1)] = inf;
        d[at(1, j + 1)] = j;
    }

    // Last row in which each character of `a` was seen. A BTreeMap rather
    // than an array because package names are not guaranteed ASCII, and the
    // alphabet of a single name is a handful of entries either way.
    let mut last_row: BTreeMap<char, usize> = BTreeMap::new();

    for i in 1..=n {
        let mut last_col = 0usize;
        let mut row_min = usize::MAX;
        for j in 1..=m {
            let k_row = last_row.get(&b[j - 1]).copied().unwrap_or(0);
            let l_col = last_col;
            let cost = usize::from(a[i - 1] != b[j - 1]);
            if cost == 0 {
                last_col = j;
            }
            let transpose = d[at(k_row, l_col)]
                .saturating_add(i - k_row - 1)
                .saturating_add(1)
                .saturating_add(j - l_col - 1);
            let cell = (d[at(i, j)] + cost)
                .min(d[at(i + 1, j)] + 1)
                .min(d[at(i, j + 1)] + 1)
                .min(transpose);
            d[at(i + 1, j + 1)] = cell;
            row_min = row_min.min(cell);
        }
        last_row.insert(a[i - 1], i);
        if row_min > k {
            return None;
        }
    }

    let total = d[at(n + 1, m + 1)];
    (total <= k).then_some(total)
}
