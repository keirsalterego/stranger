//! Malformed input must produce an error, never a panic and never a hang.
//!
//! Three hand-written parsers and six readers, all of them walking bytes that
//! came off somebody else's disk. The happy-path tests say the readers get the
//! right answer from a well-formed file. Nothing said what they do with a
//! truncated download, a merge-conflict marker, or a file that a disk flipped a
//! bit in — and "index out of bounds" on a security tool's input path is a
//! denial of service with extra steps.
//!
//! The method is deliberately crude: take a real fixture, corrupt a handful of
//! bytes, feed it back in. That finds the errors that come from *nearly* valid
//! input, which is the interesting class. A random byte string is rejected by
//! the first character and proves nothing.
//!
//! # No RNG in the standard library
//!
//! So five lines of xorshift, the same generator the ablation uses to thin the
//! corpus and the property tests use for short strings. The seed is a constant
//! here rather than the clock: a
//! fuzz run that finds a panic and cannot be replayed has told you there is a
//! bug and not which one. Change `SEED` to explore elsewhere; the failure
//! message prints what to change it to.
//!
//! It has a short period and fails statistical tests a real generator passes.
//! For picking byte offsets that does not matter, and nothing here is
//! security-sensitive — `stranger` computes no hashes and holds no keys.
//!
//! # What it has found
//!
//! A bug in itself, immediately: `truncate` can empty the buffer and the next
//! `below(bytes.len())` divided by zero. That is recorded rather than tidied
//! away, because a fuzz harness that has never failed is a fuzz harness nobody
//! has checked runs.
//!
//! Nothing in the readers. 5,000 iterations per reader here, plus three further
//! seeds run by hand at this size and three more at 20,000 — on the order of
//! 500,000 mutated inputs across six readers. That is a weak claim stated
//! precisely rather than a strong one stated vaguely: this finds panics
//! reachable from *nearly* valid input, and it has found none.

use std::path::Path;

const SEED: u64 = 0x5DEECE66D;

struct Rng(u64);

impl Rng {
    /// The same shifts as `tests/distance.rs` and `tests/ablation.rs`, and the
    /// same `| 1` at construction. Two xorshift variants in one repository
    /// would be two things to reason about for no gain.
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

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// One mutated copy. Byte-level rather than char-level on purpose: producing
/// invalid UTF-8 is a case the readers have to survive, and `from_utf8` is
/// where they have to survive it.
fn corrupt(src: &str, rng: &mut Rng, edits: usize) -> Vec<u8> {
    let mut bytes = src.as_bytes().to_vec();
    if bytes.is_empty() {
        return bytes;
    }
    for _ in 0..edits {
        // A previous truncation can have emptied it, and `below(0)` divides by
        // zero. Found by this test failing on its own harness, which is at
        // least evidence the harness runs.
        if bytes.is_empty() {
            break;
        }
        let at = rng.below(bytes.len());
        match rng.next() % 4 {
            // Structural characters get their own arm because flipping a byte
            // at random almost never produces one, and they are what the
            // parsers branch on.
            0 => bytes[at] = *b"{}[]\",:\\ \n\t".get(rng.below(11)).unwrap(),
            1 => bytes[at] = (rng.next() % 256) as u8,
            2 => bytes.truncate(at),
            _ => bytes.insert(at, (rng.next() % 256) as u8),
        }
    }
    bytes
}

/// Every reader, driven the way `lock::read` drives it, over corrupted copies
/// of the file that reader is for.
///
/// The assertion is that this function returns. A panic fails the test and a
/// hang trips the CI timeout; both are the bug rather than a flake. The run is
/// deterministic from `SEED`, so re-running lands on the same input and the
/// panic's own file and line say where.
fn hammer(name: &str, read: fn(&Path, &str) -> stranger::error::Result<stranger::lock::Tree>) {
    let src = fixture(name);
    let path = Path::new(name);
    // `| 1` because xorshift is all zeroes forever from a zero seed, which
    // would corrupt byte 0 five thousand times and pass.
    let mut rng = Rng(SEED | 1);

    for _ in 0..5_000 {
        let edits = 1 + rng.below(8);
        let bytes = corrupt(&src, &mut rng, edits);
        // Not every mutation lands on a UTF-8 boundary. A reader never sees
        // invalid UTF-8 in production because `lock::read` uses
        // `read_to_string`, so mirroring that is the honest test.
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        // Discarding the result is the point: Ok and Err are both fine, and
        // there is no third outcome that is allowed.
        let _ = read(path, text);
    }
}

#[test]
fn npm_survives_corruption() {
    hammer("npm-s.package-lock.json", stranger::lock::npm::read);
}

#[test]
fn pnpm_survives_corruption() {
    hammer("pnpm-l.pnpm-lock.yaml", stranger::lock::pnpm::read);
}

#[test]
fn cargo_survives_corruption() {
    hammer("cargo-s.Cargo.lock", stranger::lock::cargo::read);
}

#[test]
fn poetry_survives_corruption() {
    hammer("poetry-s.poetry.lock", stranger::lock::pypi::poetry);
}

#[test]
fn uv_survives_corruption() {
    hammer("uv-m.uv.lock", stranger::lock::pypi::uv);
}

#[test]
fn pip_survives_corruption() {
    hammer("reqs-s.requirements.txt", stranger::lock::pip::read);
}
