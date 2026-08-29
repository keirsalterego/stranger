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

/// Names further apart than this are not typos of each other, they are
/// different packages.
///
/// Two is the smallest threshold that still finds every planted name in the
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

/// Distance if it is at most `k`, `None` otherwise.
///
/// The length check first is not just an optimisation: a distance of `k`
/// cannot change a string's length by more than `k`, so this is exact, and it
/// rejects the overwhelming majority of the corpus — 140,066 names for npm,
/// 15,000 for PyPI, 5,000 for crates.io — before any table gets allocated.
pub fn within(a: &str, b: &str, k: usize) -> Option<usize> {
    let (la, lb) = (a.chars().count(), b.chars().count());
    if la.abs_diff(lb) > k {
        return None;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    bounded(&a, &b, k)
}

pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let ceiling = a.len() + b.len();
    bounded(&a, &b, ceiling).unwrap_or(ceiling)
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
fn bounded(a: &[char], b: &[char], k: usize) -> Option<usize> {
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return (m <= k).then_some(m);
    }
    if m == 0 {
        return (n <= k).then_some(n);
    }

    let inf = n + m;
    let w = m + 2;
    let mut d = vec![0usize; (n + 2) * w];
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
