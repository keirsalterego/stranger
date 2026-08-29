use stranger::distance::{MAX_EDIT_DISTANCE, damerau_levenshtein as d, within};

/// std has no RNG, and this needs one. Xorshift64*, seeded from the clock so
/// repeated runs explore different inputs, and the seed is printed so a
/// failure can be replayed.
struct Rng(u64);

impl Rng {
    fn seeded() -> (Self, u64) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64
            | 1;
        (Rng(seed), seed)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn name(&mut self, max: usize) -> String {
        let n = (self.next() as usize) % (max + 1);
        (0..n)
            .map(|_| (b'a' + (self.next() % 4) as u8) as char)
            .collect()
    }
}

#[test]
fn basics() {
    assert_eq!(d("", ""), 0);
    assert_eq!(d("abc", "abc"), 0);
    assert_eq!(d("", "abc"), 3);
    assert_eq!(d("abc", ""), 3);
    assert_eq!(d("abc", "abd"), 1); // substitution
    assert_eq!(d("abc", "ab"), 1); // deletion
    assert_eq!(d("ab", "abc"), 1); // insertion
    assert_eq!(d("ab", "ba"), 1); // transposition
}

/// The reason this file implements Lowrance-Wagner rather than the optimal
/// string alignment variant that ships as "damerau-levenshtein" nearly
/// everywhere. OSA answers 3 here, because having transposed `CA` into `AC`
/// it will not then edit inside that span. The real distance is 2:
/// CA -> AC (transpose) -> ABC (insert B).
#[test]
fn unrestricted_not_osa() {
    assert_eq!(d("CA", "ABC"), 2);
}

#[test]
fn the_names_this_tool_exists_to_catch() {
    // A transposition is distance 1 here and distance 2 under plain
    // Levenshtein. That gap is the whole argument for Damerau: the threshold
    // that catches `lodahs` under Levenshtein also catches half the registry.
    assert_eq!(d("lodahs", "lodash"), 1);
    assert_eq!(d("expres", "express"), 1);
    assert_eq!(d("chalck", "chalk"), 1);
    assert_eq!(d("python-dateutils", "python-dateutil"), 1);
    assert_eq!(d("reqwests", "reqwest"), 1);

    // And the ones it must not catch.
    assert!(d("requests-http", "requests") > MAX_EDIT_DISTANCE);
    assert!(d("lodash", "express") > MAX_EDIT_DISTANCE);
}

/// Real sibling packages that a naive threshold flags and should not.
#[test]
fn legitimate_siblings_are_close_together() {
    // These are all real npm packages. The rule cannot lean on distance
    // alone, which is exactly why there is an in-degree clause.
    assert!(d("lodash.merge", "lodash.mergewith") <= 4);
    assert_eq!(d("chalk", "chalk"), 0);
}

#[test]
fn within_agrees_with_the_unbounded_version() {
    let (mut rng, seed) = Rng::seeded();
    for _ in 0..20_000 {
        let a = rng.name(8);
        let b = rng.name(8);
        let full = d(&a, &b);
        for k in 0..4 {
            let bounded = within(&a, &b, k);
            if full <= k {
                assert_eq!(bounded, Some(full), "seed={seed} a={a:?} b={b:?} k={k}");
            } else {
                assert_eq!(bounded, None, "seed={seed} a={a:?} b={b:?} k={k}");
            }
        }
    }
}

#[test]
fn identity_and_symmetry() {
    let (mut rng, seed) = Rng::seeded();
    for _ in 0..20_000 {
        let a = rng.name(10);
        let b = rng.name(10);
        assert_eq!(d(&a, &a), 0, "seed={seed}");
        assert_eq!(d(&a, &b), d(&b, &a), "seed={seed} a={a:?} b={b:?}");
    }
}

/// The property OSA fails. It holds here because this is the unrestricted
/// variant, which is an actual metric.
#[test]
fn triangle_inequality() {
    let (mut rng, seed) = Rng::seeded();
    for _ in 0..20_000 {
        let a = rng.name(7);
        let b = rng.name(7);
        let c = rng.name(7);
        assert!(
            d(&a, &c) <= d(&a, &b) + d(&b, &c),
            "seed={seed} a={a:?} b={b:?} c={c:?}"
        );
    }
}

#[test]
fn non_ascii_names_do_not_panic() {
    assert_eq!(d("café", "cafe"), 1);
    assert_eq!(d("π", "π"), 0);
    assert_eq!(d("🦀🦀", "🦀"), 1);
}

fn corpus(file: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every corpus claim in the `MAX_EDIT_DISTANCE` comment, checked.
///
/// The comment used to name `logass`, `nodash` and `loda` as npm entries that
/// a threshold of three newly reaches. Not one of the three is in the corpus,
/// and all three are already inside the current threshold, so the sentence was
/// wrong in both halves and nothing in the test suite noticed for weeks. A
/// number a comment quotes about the corpus is a number a test can hold it to,
/// so: a corpus refresh that moves these counts is meant to fail here and send
/// whoever ran it back to the comment.
#[test]
fn the_threshold_comment_is_still_true() {
    let npm = corpus("npm.txt");
    let npm: Vec<&str> = npm.lines().collect();
    let pypi = corpus("pypi.txt");
    let pypi: Vec<&str> = pypi.lines().collect();

    for (name, distance) in [("logass", 2), ("nodash", 1), ("loda", 2)] {
        assert_eq!(d(name, "lodash"), distance, "{name} vs lodash");
        assert!(distance <= MAX_EDIT_DISTANCE, "{name} is already caught");
        assert!(!npm.contains(&name), "{name} is not an npm package");
    }

    // Neighbours excluding the name itself, which is the figure the comment
    // quotes. `within` is what the corpus scan actually calls.
    let count = |names: &[&str], q: &str, k: usize| {
        names
            .iter()
            .filter(|&&n| n != q && within(q, n, k).is_some())
            .count()
    };
    let at = |names: &[&str], q: &'static str| [1, 2, 3, 4].map(|k| count(names, q, k));
    assert_eq!(at(&npm, "lodash"), [1, 6, 49, 467]);
    assert_eq!(at(&npm, "express"), [2, 4, 44, 145]);
    assert_eq!(at(&pypi, "requests"), [1, 2, 5, 22]);

    // The floor. This is a planted name in `poisoned.requirements.txt` and a
    // true positive, and a threshold of one loses it.
    assert_eq!(d("requests-http", "requests-html"), 2);
}
