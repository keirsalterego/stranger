#!/usr/bin/env sh
# Every lockfile on this machine, through the reader for it.
#
# The fixtures are 23 files chosen partly because they are interesting. A
# developer's disk is a few thousand files chosen by nobody, which is a
# different question: not "does the reader handle the hard case" but "does it
# handle the ordinary one". Both are worth asking and only one of them is in
# `cargo test`.
#
# This is the harness that found four bugs in an afternoon, all of them in the
# ordinary case and none of them reachable from the fixtures:
#
#   yarn   a bare `peerDependencies:` header refused the file
#   yaml   a `deprecated: |-` block scalar refused the file
#   gomod  a quoted module path refused the file — gopkg.in/yaml.v3 ships one
#   diff   a finding moving without a package printed "no change" and exited 1
#
#   ./scripts/sweep.sh              # $HOME
#   ./scripts/sweep.sh /src /opt    # somewhere else
#
# A *refusal* is a pass: a lockfileVersion this tool does not read, or a Berry
# file wearing yarn's name, are answers. Only a file the reader could not get
# through counts against it, and finding one is the point.
set -eu
cd "$(dirname "$0")/.."

BIN=./target/release/stranger
[ -x "$BIN" ] || cargo build --release --quiet

list=$(mktemp)
trap 'rm -f "$list"' EXIT

# The repository's own target/ holds deliberately malformed lockfiles written
# by tests/cli.rs. They are supposed to fail and they are not evidence.
for root in "${@:-$HOME}"; do
  find "$root" -type f \( \
    -name package-lock.json -o -name pnpm-lock.yaml -o -name yarn.lock \
    -o -name Cargo.lock -o -name poetry.lock -o -name uv.lock \
    -o -name go.mod -o -name requirements.txt \
    \) -size +0 2>/dev/null
done | grep -v "^$(pwd)/" | sort -u > "$list"

scanned=0
refused=0
unread=0

# Redirected rather than piped, so the counters survive the loop.
while read -r f; do
  scanned=$((scanned + 1))
  out=$("$BIN" scan "$f" 2>&1) || true
  case "$out" in
    *panicked*)
      unread=$((unread + 1))
      echo "PANIC   $f"
      echo "        $(echo "$out" | head -1)"
      ;;
    "stranger: "*"is not supported"*|"stranger: "*"Yarn Berry"*)
      refused=$((refused + 1))
      ;;
    "stranger: "*)
      unread=$((unread + 1))
      echo "UNREAD  $f"
      echo "        $(echo "$out" | head -1 | sed 's/^stranger: //')"
      ;;
  esac
done < "$list"

echo
echo "scanned  $scanned lockfiles"
echo "refused  $refused (a version this tool does not read — an answer, not a failure)"
echo "unread   $unread"

[ "$unread" -eq 0 ] || exit 1
