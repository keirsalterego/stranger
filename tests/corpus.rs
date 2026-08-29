use stranger::corpus;
use stranger::distance::{self, Query, budget_for};
use stranger::lock::Ecosystem;

const ALL: &[Ecosystem] = &[Ecosystem::Npm, Ecosystem::PyPi, Ecosystem::Crates];

/// `nearest_in` is the real seam — the rules pass their own list so
/// `tests/ablation.rs` can shrink it. These tests want the compiled-in list,
/// which is one call away; a wrapper in `src/` that only tests reached would
/// be library surface paid for by nobody.
fn nearest(eco: Ecosystem, name: &str) -> Option<(&'static str, usize)> {
    corpus::nearest_in(corpus::names(eco), eco, name)
}

/// `binary_search` on an unsorted slice does not fail loudly, it just returns
/// the wrong answer for some inputs. Shell `sort` is locale-dependent, so the
/// files are generated with `LC_ALL=C` and checked here rather than trusted.
#[test]
fn every_corpus_is_sorted_in_byte_order() {
    for &eco in ALL {
        let names = corpus::names(eco);
        assert!(!names.is_empty(), "{} corpus is empty", eco.as_str());
        assert!(
            names.windows(2).all(|w| w[0] < w[1]),
            "{} corpus is not sorted and deduplicated",
            eco.as_str()
        );
    }
}

#[test]
fn names_are_already_normalised() {
    for &eco in ALL {
        for name in corpus::names(eco) {
            assert_eq!(
                &corpus::normalize(eco, name),
                name,
                "{} holds {name:?} unnormalised",
                eco.as_str()
            );
        }
    }
}

#[test]
fn sizes() {
    assert_eq!(corpus::names(Ecosystem::Crates).len(), 5_000);
    assert_eq!(corpus::names(Ecosystem::PyPi).len(), 15_000);
    assert_eq!(corpus::names(Ecosystem::Npm).len(), 140_066);
    // No ranked list of Go module paths exists, so the rule never fires there.
    // Empty on purpose, and said out loud in README LIMITS.
    assert!(corpus::names(Ecosystem::Go).is_empty());
}

/// Go is read and Go is not checked, and those are two facts rather than one.
/// This test used to assert the first half away — no `go.mod` in `KNOWN`, so
/// nothing could reach an empty corpus. A reader landed, so the half that has
/// to keep holding is the second: `tests/gomod.rs` proves the rule stays
/// silent, and this proves why it has to.
#[test]
fn go_is_read_and_still_has_no_corpus() {
    assert!(stranger::lock::KNOWN.contains(&"go.mod"));
    assert!(
        corpus::names(Ecosystem::Go).is_empty(),
        "a Go corpus appeared: README LIMITS and the slopsquat guard both claim there is none"
    );
}

/// The two-letter registry sweep missed `lodash`, which would have made the
/// tool report one of the most depended-upon packages on npm as a
/// hallucination. These are the names that gap was caught with.
#[test]
fn popular_packages_are_present() {
    for name in [
        "lodash",
        "chalk",
        "express",
        "react",
        "typescript",
        "webpack",
        "eslint",
    ] {
        assert!(
            corpus::contains(Ecosystem::Npm, name),
            "npm is missing {name}"
        );
    }
    for name in ["requests", "python-dateutil", "numpy", "flask", "urllib3"] {
        assert!(
            corpus::contains(Ecosystem::PyPi, name),
            "pypi is missing {name}"
        );
    }
    for name in ["serde_json", "clap", "toml", "walkdir", "strsim"] {
        assert!(
            corpus::contains(Ecosystem::Crates, name),
            "crates.io is missing {name}"
        );
    }
}

#[test]
fn planted_names_are_absent() {
    for name in ["expres", "lodahs", "chalck"] {
        assert!(
            !corpus::contains(Ecosystem::Npm, name),
            "npm unexpectedly has {name}"
        );
    }
    for name in ["python-dateutils", "requests-http"] {
        assert!(
            !corpus::contains(Ecosystem::PyPi, name),
            "pypi unexpectedly has {name}"
        );
    }
}

/// PEP 503: PyPI treats these as one project, so a separator choice must not
/// read as a typo.
#[test]
fn pypi_separators_are_equivalent() {
    for spelling in [
        "python-dateutil",
        "python_dateutil",
        "Python.DateUtil",
        "python--dateutil",
    ] {
        assert!(corpus::contains(Ecosystem::PyPi, spelling), "{spelling}");
    }
}

#[test]
fn nearest_finds_the_obvious_parent() {
    assert_eq!(nearest(Ecosystem::Npm, "expres"), Some(("express", 1)));
    assert_eq!(nearest(Ecosystem::Npm, "lodahs"), Some(("lodash", 1)));
    assert_eq!(nearest(Ecosystem::Npm, "chalck"), Some(("chalk", 1)));
    assert_eq!(
        nearest(Ecosystem::PyPi, "python-dateutils"),
        Some(("python-dateutil", 1))
    );
}

/// The bug `CHARS_PER_EDIT` exists for.
///
/// Every one of these is a real package sitting below its registry's
/// popularity cut, and every one of them used to come out CRITICAL: a name
/// this short is two edits from something in any registry, so clause two
/// passed for free and the rule quietly became a two-clause rule. The parents
/// it named are the tell — `am`, `lr`, `ci`, `h2` are junk short entries that
/// won the shorter-name tie-break, and nothing about them is a plausible thing
/// for a model to have mistyped.
#[test]
fn short_names_do_not_all_look_like_typos() {
    for (eco, name) in [
        (Ecosystem::PyPi, "hy"),     // the Hy lisp, on PyPI since 2013
        (Ecosystem::Crates, "iced"), // d=2 from `cid`
        (Ecosystem::Npm, "lru"),     // d=1 from `lr`
        (Ecosystem::Npm, "vm"),      // d=1 from `am`
        (Ecosystem::Npm, "fkill"),   // d=2 from `quill`
        (Ecosystem::Crates, "ksni"), // d=2 from `jni`
        (Ecosystem::Npm, "taze"),    // d=1 from `gaze`
    ] {
        assert!(!corpus::contains(eco, name), "{name} is in the corpus now");
        assert_eq!(nearest(eco, name), None, "{name} still names a parent");
    }

    // And the floor it must not cross. Six characters buys one edit, which is
    // every planted name in the fixtures.
    assert_eq!(nearest(Ecosystem::Npm, "expres"), Some(("express", 1)));
    assert_eq!(nearest(Ecosystem::PyPi, "nunpy"), Some(("numpy", 1)));
}

/// Every candidate the bucketed sweep skipped, it was entitled to skip.
///
/// `ByLength::nearest` only looks at the buckets within `k` of the query's
/// length. The bound is exact — an edit moves a length by at most one — and it
/// is also the one thing in `corpus.rs` that could be wrong without anything
/// else noticing: an off-by-one in the range makes the rule quietly miss
/// names, no fixture fails, no count moves, and a detection tool becomes a
/// quiet one. So: the exhaustive sweep it replaced, over the whole list, name
/// for name and distance for distance.
#[test]
fn bucketing_skips_nothing_an_exhaustive_sweep_would_find() {
    for &eco in ALL {
        let names = corpus::names(eco);
        for probe in probes(names) {
            assert_eq!(
                corpus::nearest_in(names, eco, &probe),
                exhaustive(names, eco, &probe),
                "{} disagreed on {probe:?}",
                eco.as_str()
            );
        }
    }
}

/// The sweep `ByLength` replaced: every name in the list, no bucket range.
fn exhaustive(names: &[&'static str], eco: Ecosystem, name: &str) -> Option<(&'static str, usize)> {
    let query = corpus::normalize(eco, name);
    let k = budget_for(query.chars().count());
    if k == 0 {
        return None;
    }
    names
        .iter()
        .filter_map(|&c| distance::within(&query, c, k).map(|d| (c, d)))
        .min_by_key(|&(c, d)| (d, c.len(), c))
}

/// Near-misses spread across the whole list: every 4,001st name with one
/// character deleted, doubled or transposed. A fixed stride rather than a
/// seeded sample, because a corpus sweep is slow enough that a flaky version
/// of this would get deleted rather than debugged.
fn probes(names: &[&'static str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate().step_by(4_001) {
        let mut chars: Vec<char> = name.chars().collect();
        let at = i % chars.len();
        match i % 3 {
            0 => {
                chars.remove(at);
            }
            1 => chars.insert(at, chars[at]),
            _ => {
                let last = chars.len() - 1;
                chars.swap(at, if at == last { 0 } else { at + 1 });
            }
        }
        out.push(chars.into_iter().collect());
    }
    // The names the rule exists to catch, the two it used to invent, and a
    // name longer than anything in any corpus — the bucket range runs off the
    // end of the index there and has to come back empty rather than panic.
    out.extend(["expres", "lodahs", "chalck", "ksni", "taze", "hy", "a", ""].map(String::from));
    out.push("z".repeat(400));
    out
}

/// Where `distance::CHARS_PER_EDIT` comes from: the share of names that find
/// some neighbour when you pretend they are missing.
///
/// A real package below the popularity cut is exactly a corpus name with the
/// corpus name removed, so leave-one-out is the false positive rate for clause
/// two, per length, per registry. Ignored by default — it is an all-pairs
/// sweep of three corpora and takes about ten minutes.
///
/// `cargo test --release --test corpus -- --ignored --nocapture` prints it.
#[test]
#[ignore = "slow; ten minutes of all-pairs sweep"]
fn length_is_the_false_positive_rate() {
    for &eco in ALL {
        let names = corpus::names(eco);
        let buckets = by_length(names);
        println!(
            "\n## {} — {} names, leave-one-out\n",
            eco.as_str(),
            names.len()
        );
        println!("| chars | in corpus | sampled | k=1 | k=2 |");
        println!("|---|---|---|---|---|");
        for (len, list) in buckets.iter().enumerate().take(21).skip(1) {
            if list.is_empty() {
                continue;
            }
            // Exhaustive where the answer is interesting, a 1-in-20 stride
            // past nine characters where it is already near the floor.
            let step = if len <= 9 { 1 } else { 20 };
            let sample: Vec<&str> = list.iter().copied().step_by(step).collect();
            let hits = |k: usize| {
                sample
                    .iter()
                    .filter(|&&n| has_neighbour(&buckets, n, k))
                    .count() as f64
                    * 100.0
                    / sample.len() as f64
            };
            println!(
                "| {len} | {} | {} | {:.1}% | {:.1}% |",
                list.len(),
                sample.len(),
                hits(1),
                hits(2)
            );
        }
    }
}

fn by_length(names: &[&'static str]) -> Vec<Vec<&'static str>> {
    let max = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let mut b = vec![Vec::new(); max + 1];
    for &n in names {
        b[n.chars().count()].push(n);
    }
    b
}

/// Any neighbour within `k` other than `q` itself, which is all the rate
/// question needs — stops at the first.
fn has_neighbour(b: &[Vec<&'static str>], q: &str, k: usize) -> bool {
    let mut query = Query::with_budget(q, k);
    let lo = query.char_len().saturating_sub(k);
    let hi = (query.char_len() + k).min(b.len() - 1);
    match b.get(lo..=hi) {
        None => false,
        Some(range) => range
            .iter()
            .flatten()
            .any(|&c| c != q && query.distance_to(c).is_some()),
    }
}

/// A name that is not a near-miss of anything real gets no neighbour, and the
/// rule stays quiet rather than inventing one.
#[test]
fn nearest_gives_up_on_names_that_are_not_typos() {
    assert_eq!(nearest(Ecosystem::Npm, "zzqxwvunexistentpackage"), None);
    assert_eq!(nearest(Ecosystem::Go, "github.com/whatever/thing"), None);
}
