#!/usr/bin/env sh
# Reproducible build check.
#
# Builds the same commit twice in two different directories and compares the
# sha256 of the binary. Two directories is the strong claim; the hackathon's FAQ
# sets the bar at "same machine, same toolchain, build twice", which
# same-directory-twice already meets.
#
# The three settings that matter:
#   SOURCE_DATE_EPOCH  pins anything that would otherwise embed a build time
#   CARGO_INCREMENTAL  incremental artifacts are not deterministic
#   --remap-path-prefix  the absolute build path leaks into panic messages
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
WORK=${TMPDIR:-/tmp}/stranger-repro.$$
A="$WORK/a"
B="$WORK/b-with-a-deliberately-longer-name"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$A" "$B"

# Kickoff, 2026-08-28 18:00 UTC.
export SOURCE_DATE_EPOCH=1787940000
export CARGO_INCREMENTAL=0

build() {
  dir=$1
  git -C "$REPO" archive HEAD | tar -x -C "$dir"
  ( cd "$dir" \
    && RUSTFLAGS="--remap-path-prefix=$dir=/build -C debuginfo=0" \
       cargo build --release --locked --offline >/dev/null 2>&1 )
  sha256sum "$dir/target/release/stranger" | cut -d' ' -f1
}

echo "commit:  $(git -C "$REPO" rev-parse HEAD)"
echo "rustc:   $(rustc --version)"
echo "epoch:   $SOURCE_DATE_EPOCH"
echo

HA=$(build "$A")
echo "build A  $A"
echo "         $HA"
HB=$(build "$B")
echo "build B  $B"
echo "         $HB"
echo

if [ "$HA" = "$HB" ]; then
  echo "MATCH — byte-identical across two directories"
  exit 0
fi
echo "DIFFER"
echo "Bisect order: path leakage in panic messages, then build-id, then"
echo "incremental artifacts. Compare with:"
echo "  cmp -l $A/target/release/stranger $B/target/release/stranger | head"
exit 1
