//! What the third clause is actually worth.
//!
//! The slopsquat rule is a conjunction: not in corpus, AND within
//! `MAX_EDIT_DISTANCE` of a real name, AND nothing in the tree depends on it.
//! The first two are what everyone would write. The third is the claim, so it
//! gets measured instead of asserted.
//!
//! Ground truth is the fixture set: `poisoned.package-lock.json` contains
//! exactly three planted names and every other npm fixture contains none, so
//! any finding outside the planted set is a false positive by construction.
//!
//! The first version of this file measured the clause against the full 140,066
//! name corpus and found it worth exactly nothing — both configurations scored
//! 1.000 precision and 1.000 recall. That is a real result and it is reported
//! below, but it is measuring the wrong thing. With a corpus that contains
//! every package in every fixture, clause one alone is sufficient, and no
//! other clause can show a difference.
//!
//! A corpus is never that. Mine is a snapshot taken on one afternoon; npm
//! accepts thousands of new names a day, and a package published after the
//! snapshot is indistinguishable from a package that does not exist. So the
//! question worth measuring is what happens as clause one degrades — and that
//! is what the second table does, by deleting a fraction of the corpus and
//! watching which clause is still holding the rule up.
//!
//! The third table is about clause two rather than clause three: what edit
//! budget a name's length has earned. `distance::CHARS_PER_EDIT` is read off
//! it, so it is here rather than argued in a comment somewhere.
//!
//! Run `cargo test --release --test ablation -- --nocapture` to print them.

use std::fs;
use std::path::Path;
use stranger::corpus;
use stranger::distance::Query;
use stranger::lock::{self, Origin, Tree, npm};
use stranger::rules::slopsquat::{self, Config};

const PLANTED: &[&str] = &["expres", "lodahs", "chalck"];

const FIXTURES: &[&str] = &[
    "npm-xs.package-lock.json",
    "npm-s.package-lock.json",
    "npm-m.package-lock.json",
    "npm-l.package-lock.json",
    "npm-xl.package-lock.json",
    "poisoned.package-lock.json",
];

fn load(name: &str) -> Tree {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let src = fs::read_to_string(&path).unwrap();
    npm::read(&path, &src).unwrap()
}

struct Score {
    tp: usize,
    fp: usize,
    fn_: usize,
    scanned: usize,
}

impl Score {
    fn precision(&self) -> f64 {
        if self.tp + self.fp == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fp) as f64
        }
    }
    fn recall(&self) -> f64 {
        if self.tp + self.fn_ == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fn_) as f64
        }
    }
}

/// Deterministic corpus thinning. Keeps roughly `keep_permille` of the names,
/// chosen by a seeded xorshift so the table is reproducible rather than
/// different every run. Order is preserved, so the result is still sorted and
/// `binary_search` still holds.
fn thinned(names: &[&'static str], keep_permille: u64, seed: u64) -> Vec<&'static str> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(names.len());
    for &n in names {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        if state % 1000 < keep_permille {
            out.push(n);
        }
    }
    out
}

fn measure(cfg: Config<'_>) -> (Score, Vec<String>) {
    let mut s = Score {
        tp: 0,
        fp: 0,
        fn_: 0,
        scanned: 0,
    };
    let mut false_positives = Vec::new();

    for name in FIXTURES {
        let tree = load(name);
        s.scanned += tree.packages.len();
        let found: Vec<String> = slopsquat::scan(&tree, cfg)
            .into_iter()
            .map(|f| f.package)
            .collect();

        for f in &found {
            if PLANTED.contains(&f.as_str()) {
                s.tp += 1;
            } else {
                s.fp += 1;
                false_positives.push(format!("{name}: {f}"));
            }
        }
        if name.starts_with("poisoned") {
            s.fn_ += PLANTED
                .iter()
                .filter(|p| !found.contains(&p.to_string()))
                .count();
        }
    }
    (s, false_positives)
}

#[test]
fn ablation_table() {
    let (with, with_fps) = measure(Config {
        require_no_parent: true,
        corpus: None,
    });
    let (without, without_fps) = measure(Config {
        require_no_parent: false,
        corpus: None,
    });

    println!();
    println!("packages scanned: {}", with.scanned);
    println!("planted true positives available: {}", PLANTED.len());
    println!();
    println!("| in-degree clause | TP | FP | FN | precision | recall |");
    println!("|---|---|---|---|---|---|");
    for (label, s) in [("on (shipped)", &with), ("off (ablated)", &without)] {
        println!(
            "| {label} | {} | {} | {} | {:.3} | {:.3} |",
            s.tp,
            s.fp,
            s.fn_,
            s.precision(),
            s.recall()
        );
    }
    println!();
    if !without_fps.is_empty() {
        println!(
            "false positives with the clause OFF ({}):",
            without_fps.len()
        );
        for f in &without_fps {
            println!("  {f}");
        }
    }
    if !with_fps.is_empty() {
        println!("false positives with the clause ON ({}):", with_fps.len());
        for f in &with_fps {
            println!("  {f}");
        }
    }
    println!();

    // The shipped configuration must find every planted name.
    assert_eq!(
        with.tp,
        PLANTED.len(),
        "shipped config missed a planted name"
    );
    assert_eq!(with.fn_, 0);
}

/// The table that actually says something.
///
/// Ignored by default because it scans the fixtures ten times against a
/// 140,000-name corpus and takes about two minutes. `make ablation` runs it.
#[test]
#[ignore = "slow; run with `make ablation`"]
fn ablation_under_corpus_decay() {
    let full = stranger::corpus::names(stranger::lock::Ecosystem::Npm);
    const SEED: u64 = 0x5EED_1234;

    println!();
    println!("corpus decay — npm, {} names at 100%", full.len());
    println!();
    println!("| corpus kept | in-degree clause | TP | FP | precision | recall |");
    println!("|---|---|---|---|---|---|");

    let mut rows = Vec::new();
    for keep in [1000u64, 900, 700, 500, 250] {
        let subset = thinned(full, keep, SEED);
        for (label, require) in [("on", true), ("off", false)] {
            let cfg = Config {
                require_no_parent: require,
                corpus: Some(&subset),
            };
            let (s, _) = measure(cfg);
            println!(
                "| {}% ({}) | {label} | {} | {} | {:.3} | {:.3} |",
                keep / 10,
                subset.len(),
                s.tp,
                s.fp,
                s.precision(),
                s.recall()
            );
            rows.push((keep, require, s.fp, s.tp));
        }
    }
    println!();

    // At every level of decay, keeping the clause must cost no recall and must
    // never produce more false positives than dropping it.
    for keep in [1000u64, 900, 700, 500, 250] {
        let on = rows.iter().find(|r| r.0 == keep && r.1).unwrap();
        let off = rows.iter().find(|r| r.0 == keep && !r.1).unwrap();
        assert!(
            on.2 <= off.2,
            "clause made false positives worse at {keep}permille"
        );
        assert_eq!(on.3, off.3, "clause cost a true positive at {keep}permille");
    }
}

// ------------------------------------------------------- clause two's budget

/// The other three planted names, on the other two fixtures that carry any.
/// `hostile.package-lock.json` repeats `lodahs` and `chalck` — its point is the
/// renderer, but the names are the same invented ones and they count.
const PLANTED_ALL: &[&str] = &[
    "expres",
    "lodahs",
    "chalck",
    "python-dateutils",
    "requests-http",
];

/// Seven planted findings, not five: three in `poisoned.package-lock.json`,
/// two in `poisoned.requirements.txt`, and `lodahs` and `chalck` a second time
/// in `hostile`.
const PLANTED_TOTAL: usize = 7;

/// Every fixture, not just the npm ones: the question is about name length,
/// and the three corpora have different length distributions.
const EVERY_FIXTURE: &[&str] = &[
    "cargo-l.Cargo.lock",
    "cargo-m.Cargo.lock",
    "cargo-s.Cargo.lock",
    "gomod-m.go.mod",
    "gomod-xs.go.mod",
    "hostile.package-lock.json",
    "npm-l.package-lock.json",
    "npm-m.package-lock.json",
    "npm-s.package-lock.json",
    "npm-xl.package-lock.json",
    "npm-xs.package-lock.json",
    "pnpm-l.pnpm-lock.yaml",
    "poetry-m.poetry.lock",
    "poetry-s.poetry.lock",
    "poisoned.package-lock.json",
    "poisoned.requirements.txt",
    "reqs-s.requirements.txt",
    "reqs-xs.requirements.txt",
    "uv-m.uv.lock",
];

type Budget = fn(usize) -> usize;

/// Candidate budgets, as `k` against a name of `len` characters.
///
/// The shipped policy is in here under its own name and has to come out
/// matching `slopsquat::scan`, which is what stops this table drifting into
/// measuring a rule the binary does not run.
const POLICIES: &[(&str, Budget)] = &[
    ("2 (what shipped)", |_| 2),
    ("min(2, len/3)", |l| 2.min(l / 3)),
    ("min(2, (len-1)/3)", |l| 2.min(l.saturating_sub(1) / 3)),
    ("min(2, len/4)", |l| 2.min(l / 4)),
    ("min(2, (len-1)/4)", |l| 2.min(l.saturating_sub(1) / 4)),
    ("min(2, len/5) — shipped", stranger::distance::budget_for),
    ("min(2, (len-1)/5)", |l| 2.min(l.saturating_sub(1) / 5)),
    ("min(2, len/6)", |l| 2.min(l / 6)),
    ("1 (a tighter ceiling)", |_| 1),
];

/// The three clauses with the budget as a parameter.
///
/// A copy of `slopsquat::scan`, which is a thing to be suspicious of — so
/// `the_sweep_measures_the_rule_that_ships` puts the shipped row against the
/// real rule and fails if they ever say different things.
fn fire(tree: &Tree, budget: fn(usize) -> usize) -> Vec<String> {
    let names = corpus::names(tree.ecosystem);
    if names.is_empty() {
        return Vec::new();
    }
    let buckets = by_length(names);
    let deg = tree.in_degree();
    let mut out = Vec::new();
    for (i, pkg) in tree.packages.iter().enumerate() {
        if pkg.first_party || pkg.origin == Origin::Elsewhere || deg[i] > 0 {
            continue;
        }
        if corpus::contains_in(names, tree.ecosystem, &pkg.name) {
            continue;
        }
        let q = corpus::normalize(tree.ecosystem, &pkg.name);
        if nearest(&buckets, &q, budget(q.chars().count())).is_some() {
            out.push(pkg.name.clone());
        }
    }
    out.sort();
    out
}

fn by_length(names: &[&'static str]) -> Vec<Vec<&'static str>> {
    let max = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let mut b = vec![Vec::new(); max + 1];
    for &n in names {
        b[n.chars().count()].push(n);
    }
    b
}

fn nearest(b: &[Vec<&'static str>], q: &str, k: usize) -> Option<(&'static str, usize)> {
    if k == 0 {
        return None;
    }
    let mut query = Query::with_budget(q, k);
    let lo = query.char_len().saturating_sub(k);
    let hi = (query.char_len() + k).min(b.len() - 1);
    let mut best: Option<(&'static str, usize)> = None;
    for &c in b.get(lo..=hi)?.iter().flatten() {
        if let Some(d) = query.distance_to(c)
            && best.is_none_or(|(bn, bd)| (d, c.len(), c) < (bd, bn.len(), bn))
        {
            best = Some((c, d));
        }
    }
    best
}

fn every_tree() -> Vec<(&'static str, Tree)> {
    EVERY_FIXTURE
        .iter()
        .map(|&f| {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(f);
            (f, lock::read(&path).unwrap_or_else(|e| panic!("{f}: {e}")))
        })
        .collect()
}

/// Where `distance::CHARS_PER_EDIT` comes from.
///
/// `MAX_EDIT_DISTANCE` was absolute, and for a three-character name two edits
/// reach something in every registry — so clause two passed for free and the
/// rule was a two-clause rule for every short name in it. This sweeps the
/// candidate policies over every fixture and prints what each one costs.
///
/// The bar is precision at no recall: losing a planted name is not a trade,
/// at any price. `min(2, (len-1)/4)`, `min(2, (len-1)/5)` and `min(2, len/6)`
/// tie the shipped policy on this table at 7/1; the leave-one-out measurement
/// in `tests/corpus.rs` is what separates them, and `nunpy` in `tests/pip.rs`
/// is the true positive the last two of them lose.
#[test]
fn edit_budget_policy_sweep() {
    let trees = every_tree();

    println!();
    print!("| policy |");
    for l in 1..=16 {
        print!(" {l} |");
    }
    println!("\n|---|{}", "---|".repeat(16));
    for (label, p) in POLICIES {
        print!("| {label} |");
        for l in 1..=16 {
            print!(" {} |", p(l));
        }
        println!();
    }

    println!();
    println!("| policy | TP | FP | recall | precision | false positives |");
    println!("|---|---|---|---|---|---|");
    let mut rows = Vec::new();
    for (label, p) in POLICIES {
        let (mut tp, mut fp) = (0usize, 0usize);
        let mut names = Vec::new();
        for (file, tree) in &trees {
            for name in fire(tree, *p) {
                if PLANTED_ALL.contains(&name.as_str()) {
                    tp += 1;
                } else {
                    fp += 1;
                    names.push(format!("{name} [{file}]"));
                }
            }
        }
        let precision = tp as f64 / (tp + fp).max(1) as f64;
        println!(
            "| `{label}` | {tp} | {fp} | {:.3} | {precision:.3} | {} |",
            tp as f64 / PLANTED_TOTAL as f64,
            names.join(", ")
        );
        rows.push((*label, tp, fp));
    }
    println!();

    let shipped = rows
        .iter()
        .find(|(l, ..)| l.ends_with("shipped"))
        .expect("the shipped policy is in POLICIES");
    assert_eq!(
        shipped.1, PLANTED_TOTAL,
        "the shipped policy lost a planted name"
    );
    // Every policy that keeps full recall must beat the flat threshold, or
    // there was nothing to fix. The shipped one must be at least as good as
    // any of them.
    for (label, tp, fp) in &rows {
        if *tp == PLANTED_TOTAL {
            assert!(
                *fp >= shipped.2,
                "{label} beats the shipped policy: {fp} false positives against {}",
                shipped.2
            );
        }
    }
}

/// The sweep reimplements the rule, so this holds the reimplementation to it.
#[test]
fn the_sweep_measures_the_rule_that_ships() {
    for (file, tree) in every_tree() {
        let real: Vec<String> = slopsquat::scan(&tree, Config::default())
            .into_iter()
            .map(|f| f.package)
            .collect();
        assert_eq!(
            fire(&tree, stranger::distance::budget_for),
            real,
            "{file}: the sweep and the rule disagree"
        );
    }
}
