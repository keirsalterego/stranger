#!/usr/bin/env sh
# The long fuzz campaign. `cargo test` runs a short version of every campaign
# in tests/fuzz.rs so that the suite stays a few seconds; this is the one the
# published numbers come from.
#
# Release, not debug: the campaigns are parser-bound and the same mutants at
# five times the rate is five times the coverage for the same wall clock. The
# test harness always unwinds, so `panic = "abort"` in the release profile does
# not apply here and a panic still fails the run rather than killing the shell.
#
#   ./scripts/fuzz.sh                 # four seeds
#   SEEDS="7 8" ./scripts/fuzz.sh     # two of your own
#
# Seeds are decimal because the harness reads them with `u64::from_str`. 2 and 3
# are in the default list on purpose: the version of this harness before
# 2026-08-30 seeded `Rng(SEED | 1)`, so those two were one run reported twice.
set -eu
cd "$(dirname "$0")/.."

SEEDS=${SEEDS:-"25214903917 2 3 4"}

for s in $SEEDS; do
  echo "=== seed $s ==="
  STRANGER_FUZZ_SEED="$s" cargo test --release --test fuzz -- \
    --ignored --exact --nocapture deep_campaign
done

# No seed reaches this one — it walks prefixes of the fixtures in order — so
# running it once per seed would be the same work four times, reported as four
# times the number.
echo "=== truncation ==="
cargo test --release --test fuzz -- --ignored --exact --nocapture deep_truncation
