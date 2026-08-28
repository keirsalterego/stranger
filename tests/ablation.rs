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
//! Run `cargo test --release --test ablation -- --nocapture` to print both.

use std::fs;
use std::path::Path;
use stranger::lock::{Tree, npm};
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
