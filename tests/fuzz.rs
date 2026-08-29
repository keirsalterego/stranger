//! Malformed input must produce an error, never a panic and never a hang.
//!
//! Three hand-written parsers and six readers, all of them walking bytes that
//! came off somebody else's disk. The happy-path tests say the readers get the
//! right answer from a well-formed file. Nothing said what they do with a
//! truncated download, a merge-conflict marker, or a file that a disk flipped a
//! bit in — and "index out of bounds" on a security tool's input path is a
//! denial of service with extra steps.
//!
//! # What the previous version of this file was actually doing
//!
//! It said 100,000 single-byte mutations. It generated 35,000 mutants of 1–8
//! edits each, mutated over `u8`, and threw away every mutant that landed
//! mid-codepoint — 54% of them, so roughly 16,000 inputs reached a parser. Two
//! of its four arms shortened the input and a third inserted one byte, so
//! nothing it produced was ever structurally larger than the fixture it
//! started from. It seeded `Rng(SEED | 1)`, which makes seeds 2 and 3 the same
//! run. And it drove the six readers only: `json::parse`, `toml::parse` and
//! `yaml::parse` were never called.
//!
//! That is the reason two parser bugs survived to the last week of the
//! project. One needs a TOML header tens of thousands of segments deep and the
//! other a YAML flow sequence with tens of thousands of items, and neither is
//! reachable by deleting bytes out of a lockfile.
//!
//! This version mutates `char`s, so a mutant is a `String` and every mutant
//! reaches a parser — the discard path is gone because the type makes it
//! unreachable. Sites are picked at delimiters three times in four rather than
//! uniformly. Three of the seven arms make the input longer, one of them by
//! repeating a chunk up to 65,536 times, which is what puts a six-figure
//! document within reach of a forty-character seed: the JSON campaign's
//! biggest mutant is 917,485 characters, from a seed of 44.
//!
//! Two of the campaigns run under a much lower ceiling, and it is not modesty
//! — see `PARSERS`. Both parser bugs above are still live in this tree, both
//! are being fixed in parallel, and both take the whole test binary with them
//! when this harness reaches them: the TOML one aborts in `Drop` after the
//! parse has already returned `Ok`, and the YAML one does not finish. Those
//! two ceilings come off when the fixes land.
//!
//! # What it has found
//!
//! Nothing, in 1,356,800 mutants across four seeds and 169,834 truncation
//! prefixes, beyond the two parser bugs that were already known and are not
//! this harness's to fix. That is a weak claim stated precisely rather than a
//! strong one stated vaguely: it finds panics reachable from *nearly* valid
//! input, and outside the two shapes it is fenced off from, it has found none.
//!
//! # No RNG in the standard library
//!
//! Five lines of xorshift, the same generator `tests/distance.rs` and
//! `tests/ablation.rs` use. The seed is a constant rather than the clock: a
//! fuzz run that finds a panic and cannot be replayed has told you there is a
//! bug and not which one. `STRANGER_FUZZ_SEED` overrides it, so a second and a
//! third seed is one command rather than an edit and a rebuild, and every
//! campaign prints its seed before it runs so that a panic's captured output
//! carries the way to reproduce it.
//!
//! # Running the long campaign
//!
//!     ./scripts/fuzz.sh    # 1,356,800 mutants over four seeds, then truncation
//!
//! Two minutes and eighteen seconds in release on the machine this was written
//! on. `cargo test` runs the short version of each campaign — 16,960 mutants
//! and 2,217 prefixes, five seconds — and the long ones are `#[ignore]`d, the
//! way `tests/ablation.rs` ignores the decay ablation.

use std::fmt;
use std::path::Path;

const SEED: u64 = 0x5DEECE66D;

/// Mutation arms, in dispatch order. Named because a distribution printed as
/// seven bare integers tells the next person nothing.
const ARMS: [&str; 7] = [
    "delimiter-substitute",
    "noise-substitute",
    "truncate",
    "insert",
    "duplicate-chunk",
    "amplify",
    "delete-run",
];

/// The characters the three parsers branch on. Flipping a byte at random
/// almost never produces one, and a mutation that lands inside a version
/// string only ever produces a different version string.
const DELIMS: [char; 16] = [
    '{', '}', '[', ']', '"', '\'', ',', ':', '=', '.', '-', '#', '\\', '\n', '\t', ' ',
];

/// A code point drawn uniformly out of the 1.1 million is an unassigned CJK
/// ideograph most of the time, and all three parsers treat that as one more
/// character of a string. These are the ones that change the answer: control
/// characters RFC 8259 forbids unescaped, the byte-order mark every parser
/// here strips at offset zero and nowhere else, a non-breaking space that is
/// not YAML whitespace, and both ends of the scalar range.
const NOISE: [char; 12] = [
    '\u{0}',
    '\u{1}',
    '\u{7}',
    '\u{1b}',
    '\u{7f}',
    '\u{feff}',
    '\u{a0}',
    '\u{2028}',
    'π',
    '🦀',
    '\u{d7ff}',
    '\u{10ffff}',
];

struct Rng(u64);

impl Rng {
    /// Zero is the one seed xorshift cannot use — it stays zero forever — so
    /// zero becomes the golden-ratio constant. The old `SEED | 1` also avoided
    /// zero, and paid half the seed space for it: 2 and 3 both ran as 3, so
    /// two of the extra seeds run by hand were one run counted twice.
    fn new(seed: u64) -> Self {
        Rng(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn seed() -> u64 {
    match std::env::var("STRANGER_FUZZ_SEED") {
        Ok(s) => s.parse().expect("STRANGER_FUZZ_SEED must be a u64"),
        Err(_) => SEED,
    }
}

#[derive(Default)]
struct Stats {
    generated: usize,
    arms: [usize; ARMS.len()],
    grew: usize,
    shrank: usize,
    unchanged_length: usize,
    longest: usize,
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} mutants, {} longer / {} shorter / {} same length, longest {} chars",
            self.generated, self.grew, self.shrank, self.unchanged_length, self.longest
        )?;
        for (arm, n) in ARMS.iter().zip(&self.arms) {
            write!(f, "\n    {arm:<21} {n}")?;
        }
        Ok(())
    }
}

/// Three times out of four, mutate at a character a parser branches on.
/// Uniform selection over a 179 KB lockfile lands inside a version string or a
/// base64 integrity hash almost every time, and a fuzzer spending its budget
/// on producing a different version string is measuring nothing.
fn site(rng: &mut Rng, len: usize, delims: &[usize]) -> usize {
    if delims.is_empty() || rng.next().is_multiple_of(4) {
        rng.below(len)
    } else {
        delims[rng.below(delims.len())]
    }
}

fn noise(rng: &mut Rng) -> char {
    if rng.next().is_multiple_of(2) {
        NOISE[rng.below(NOISE.len())]
    } else {
        loop {
            if let Some(c) = char::from_u32(rng.below(0x11_0000) as u32) {
                return c;
            }
        }
    }
}

/// One mutant, as a `String`, never longer than `ceiling` chars.
///
/// Char-level rather than byte-level: the byte version produced invalid UTF-8
/// in more than half its output and `from_utf8` discarded it before any parser
/// saw it, so half of that campaign was an expensive test of
/// `std::str::from_utf8`.
fn mutate(
    base: &[char],
    delims: &[usize],
    ceiling: usize,
    rng: &mut Rng,
    stats: &mut Stats,
) -> String {
    let mut out = base.to_vec();
    let edits = 1 + rng.below(8);
    // Every site is chosen against the original and then applied highest index
    // first, so an insertion never moves a site chosen before it. Choosing
    // them one at a time against a buffer being edited underneath is where the
    // drift comes from, and drift is what undoes the delimiter targeting.
    let mut sites: Vec<usize> = (0..edits).map(|_| site(rng, base.len(), delims)).collect();
    sites.sort_unstable_by(|a, b| b.cmp(a));

    for at in sites {
        if out.is_empty() {
            break;
        }
        let at = at.min(out.len() - 1);
        let headroom = ceiling.saturating_sub(out.len());
        let arm = rng.below(ARMS.len());
        stats.arms[arm] += 1;
        match arm {
            0 => out[at] = DELIMS[rng.below(DELIMS.len())],
            1 => out[at] = noise(rng),
            2 => out.truncate(at),
            3 => {
                let c = if rng.next().is_multiple_of(2) {
                    DELIMS[rng.below(DELIMS.len())]
                } else {
                    noise(rng)
                };
                out.insert(at, c);
            }
            4 => {
                let n = (1 + rng.below(4096)).min(out.len() - at).min(headroom);
                let chunk = out[at..at + n].to_vec();
                out.splice(at..at, chunk);
            }
            5 => {
                // The arm that reaches a five-figure structure. A short chunk
                // repeated a power-of-two number of times, up to 2^16. Drawing
                // the repeat count uniformly from 1..65536 has to be lucky
                // once; drawing the exponent is lucky one time in seventeen,
                // and every deep-structure bug in this repository needed five
                // figures before it showed itself.
                let n = (1 + rng.below(8)).min(out.len() - at);
                let k = (1usize << rng.below(17)).min(headroom / n);
                let grown: Vec<char> = out[at..at + n]
                    .iter()
                    .copied()
                    .cycle()
                    .take(n * k)
                    .collect();
                out.splice(at..at, grown);
            }
            _ => {
                let n = (1 + rng.below(64)).min(out.len() - at);
                out.drain(at..at + n);
            }
        }
    }

    // The insert arm adds a char whatever the headroom said, and eight of them
    // in one mutant walked `toml::parse` four characters past a ceiling that
    // exists to keep a bug from aborting the binary. Cheaper to make the
    // ceiling true here than to make every arm careful.
    out.truncate(ceiling);

    stats.generated += 1;
    match out.len().cmp(&base.len()) {
        std::cmp::Ordering::Greater => stats.grew += 1,
        std::cmp::Ordering::Less => stats.shrank += 1,
        std::cmp::Ordering::Equal => stats.unchanged_length += 1,
    }
    stats.longest = stats.longest.max(out.len());
    out.into_iter().collect()
}

/// Drive one callee over mutants of one source.
///
/// The assertion is that `feed` returns. `Ok` and `Err` are both fine and
/// there is no third outcome that is allowed: a panic fails the test, a hang
/// trips the CI timeout, and both are the bug rather than a flake.
fn hammer(label: &str, source: &str, feed: impl Fn(&str), iters: usize, ceiling: usize) -> Stats {
    let seed = seed();
    let base: Vec<char> = source.chars().collect();
    assert!(!base.is_empty(), "{label}: nothing to mutate");
    let delims: Vec<usize> = base
        .iter()
        .enumerate()
        .filter(|(_, c)| DELIMS.contains(c))
        .map(|(i, _)| i)
        .collect();
    println!(
        "{label}: {iters} mutants, seed {seed:#x}, {} chars in, {} delimiter sites, ceiling {ceiling}",
        base.len(),
        delims.len()
    );

    // Mixed with the label so the six readers do not all run the same
    // schedule of arms against different files, which is what a shared seed
    // gives you and what made every campaign print an identical histogram.
    let mut rng = Rng::new(
        label
            .bytes()
            .fold(seed, |h, b| (h ^ b as u64).wrapping_mul(0x1000_0000_01b3)),
    );
    let mut stats = Stats::default();
    let start = std::time::Instant::now();
    for _ in 0..iters {
        feed(&mutate(&base, &delims, ceiling, &mut rng, &mut stats));
    }
    println!("{label}: {stats}\n    took {:?}", start.elapsed());
    stats
}

/// The two things the old harness could not have claimed: every arm fired, and
/// the campaign built inputs bigger than the thing it started from.
fn check(label: &str, stats: &Stats, ceiling: usize) {
    for (arm, &n) in ARMS.iter().zip(&stats.arms) {
        assert!(n > 0, "{label}: arm `{arm}` never fired");
    }
    assert!(
        stats.grew * 5 > stats.generated,
        "{label}: only {} of {} mutants were longer than the seed",
        stats.grew,
        stats.generated
    );
    // A quarter of the ceiling rather than the ceiling itself: filling a
    // 1 MB ceiling exactly needs the amplify arm to draw its longest chunk and
    // its largest exponent in the same edit, which is one draw in 136 and not
    // something a 600-mutant run should be held to.
    assert!(
        stats.longest * 4 >= ceiling,
        "{label}: the biggest mutant was {} chars against a ceiling of {ceiling}, so the \
         amplify arm is not reaching",
        stats.longest
    );
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

type Reader = fn(&Path, &str) -> stranger::error::Result<stranger::lock::Tree>;

/// Every reader `lock::read` dispatches to, against the fixture it is for.
const READERS: [(&str, Reader); 6] = [
    ("npm-s.package-lock.json", stranger::lock::npm::read),
    ("pnpm-l.pnpm-lock.yaml", stranger::lock::pnpm::read),
    ("cargo-s.Cargo.lock", stranger::lock::cargo::read),
    ("poetry-s.poetry.lock", stranger::lock::pypi::poetry),
    ("uv-m.uv.lock", stranger::lock::pypi::uv),
    ("reqs-s.requirements.txt", stranger::lock::pip::read),
];

/// How much bigger than its fixture a reader mutant may get. Growth is the
/// parsers' job below, where the seeds are forty characters and the amplify
/// arm has room to build something; a reader campaign that spends its time
/// re-parsing 1 MB copies of `uv-m.uv.lock` is buying very little per second.
const READER_HEADROOM: usize = 1 << 16;

fn feed_json(text: &str) {
    let _ = stranger::json::parse(text);
}
fn feed_toml(text: &str) {
    let _ = stranger::toml::parse(text);
}
fn feed_yaml(text: &str) {
    let _ = stranger::yaml::parse(text);
}

/// A `parse` entry point with a seed, a ceiling and a mutant count for the
/// short run.
type ParserCampaign = (&'static str, &'static str, fn(&str), usize, usize);

/// The three `parse` entry points.
///
/// The seeds are forty-odd characters each and nearly all of it is structure.
/// A 700-package `Cargo.lock` is mostly version strings and integrity hashes,
/// so amplifying a chunk of one produces a longer version string; amplifying a
/// chunk of these produces a header, a flow sequence, or a nesting level.
///
/// The two ceilings that are not 1 MB are there because of two live parser
/// bugs, both being fixed in parallel and neither introduced here:
///
/// - `yaml.rs`'s flow scanner is quadratic in the length of the flow line.
///   Measured on this machine in release: 4,000 items 34 ms, 8,000 items
///   133 ms, 16,000 items 548 ms, 32,000 items 1.30 s. The amplify arm reaches
///   500,000 items in one draw, which does not finish. 2 KB holds the worst
///   mutant near a millisecond.
/// - `toml.rs` builds one `BTreeMap` per header segment, and dropping a
///   60,000-segment table overflows the stack in `Drop` — after the parse has
///   already returned `Ok`. It is an abort, not a panic, so it takes the whole
///   test binary with it and no other test's result survives. 8 KB caps a
///   header at about 4,000 segments, which drops cleanly.
///
/// Both ceilings should go back to 1 MB when those land, and the deep campaign
/// below is where anyone checking would notice they had not.
const PARSERS: [ParserCampaign; 3] = [
    (
        "json::parse",
        "{\"a\":[1,2,{\"b\":\"c\",\"d\":[true,null,-1.5e3]}]}",
        feed_json,
        1 << 20,
        600,
    ),
    (
        "toml::parse",
        "[a]\nb = 1\nc = [1, 2]\n\n[[d]]\ne = { f = \"g\" }\n",
        feed_toml,
        1 << 13,
        10_000,
    ),
    (
        "yaml::parse",
        "a:\n  b: 1\n  c: [1, 2, 3]\nd:\n  - e: {f: g}\n  - h\n",
        feed_yaml,
        1 << 11,
        6_000,
    ),
];

/// Mutants per reader in the short run. A mutant of `uv-m.uv.lock` is a fresh
/// 718 KB copy that then gets parsed, so this is the number that keeps
/// `cargo test` in single-digit seconds. `scripts/fuzz.sh` is where the volume
/// comes from.
const QUICK_READER: usize = 60;

/// What the deep campaign multiplies the short counts by.
const DEEP: usize = 20;

/// One thread per campaign. The campaigns share nothing — separate seeds,
/// separate sources, separate parsers — and run serially they put `cargo test`
/// a minute inside this one file on an eight-core machine.
fn in_parallel<T: Sync>(items: &[T], run: impl Fn(&T) -> usize + Sync) -> usize {
    std::thread::scope(|s| {
        let handles: Vec<_> = items.iter().map(|item| s.spawn(|| run(item))).collect();
        handles
            .into_iter()
            // `resume_unwind` rather than `expect`, so a panic inside a
            // campaign arrives with its own payload instead of a message from
            // the join site saying a thread died.
            .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .sum()
    })
}

fn readers(iters: usize) -> usize {
    in_parallel(&READERS, |&(name, read)| {
        let path = Path::new(name);
        let src = fixture(name);
        let ceiling = src.chars().count() + READER_HEADROOM;
        let stats = hammer(
            name,
            &src,
            |text| {
                let _ = read(path, text);
            },
            iters,
            ceiling,
        );
        check(name, &stats, ceiling);
        stats.generated
    })
}

fn parsers(multiplier: usize) -> usize {
    in_parallel(&PARSERS, |&(label, seed, feed, ceiling, iters)| {
        let stats = hammer(label, seed, feed, iters * multiplier, ceiling);
        check(label, &stats, ceiling);
        stats.generated
    })
}

#[test]
fn readers_survive_corruption() {
    println!("readers: {} mutants", readers(QUICK_READER));
}

#[test]
fn parsers_survive_corruption() {
    println!("parsers: {} mutants", parsers(1));
}

/// Cut a real lockfile short and hand the reader the front half of it.
///
/// Truncation is the corruption that happens for a boring reason — an
/// interrupted download, a full disk, a `git checkout` killed halfway — and it
/// is the one a random mutation is worst at producing, because the truncate
/// arm has to draw a site near the end of the file to make a short prefix.
///
/// Every prefix of a 718 KB file is quadratic in the file: 258 GB of parsing
/// for one fixture. So the stride is set from `budget`, the bytes each fixture
/// is allowed to cost, which makes the six fixtures cost about the same as
/// each other however different their sizes are.
fn truncation(budget: usize) -> usize {
    in_parallel(&READERS, |&(name, read)| {
        let src = fixture(name);
        let path = Path::new(name);
        // A prefix costs half the file on average, so `len/2 * count` bytes
        // for `count` prefixes. Solving that for the budget gives the stride,
        // which is what stops `uv-m.uv.lock` from eating the whole run.
        let stride = (src.len() * src.len() / (2 * budget)).max(1);
        let mut prefixes = 0;
        for n in (0..src.len()).step_by(stride) {
            if !src.is_char_boundary(n) {
                continue;
            }
            let _ = read(path, &src[..n]);
            prefixes += 1;
        }
        println!("{name}: {prefixes} truncation prefixes, stride {stride}");
        prefixes
    })
}

#[test]
fn truncation_never_panics() {
    println!("truncation: {} prefixes", truncation(16 << 20));
}

/// The long campaign, one seed's worth. `scripts/fuzz.sh` runs four.
///
/// Separate from the short tests rather than a multiplier on them because the
/// short ones have to stay inside a `cargo test` people run on every save, and
/// this one takes minutes.
#[test]
#[ignore = "the long campaign; run with `./scripts/fuzz.sh`"]
fn deep_campaign() {
    let mutants = readers(QUICK_READER * DEEP) + parsers(DEEP);
    println!("deep campaign: {mutants} mutants, no panics");
}

/// Split from `deep_campaign` because truncation has no seed in it. Running it
/// once per seed alongside the mutation campaign would be the same 150,000
/// prefixes four times over, reported as 600,000.
#[test]
#[ignore = "the long campaign; run with `./scripts/fuzz.sh`"]
fn deep_truncation() {
    println!("deep truncation: {} prefixes", truncation(3 << 30));
}
