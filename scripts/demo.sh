#!/usr/bin/env bash
# The submission demo, driven rather than typed.
#
# Twelve beats in the order the video takes them: the empty manifest, a clone
# from GitHub that lands on the freeze tag, a build with no registry and no
# network, the poisoned fixture, the gate, both halves of clause 3, the
# ablation, and the two reproducible-build hashes.
#
# Driven because a live terminal on camera is a typo waiting to happen, and
# because every one of these has to be the real command against the real tree.
# Nothing here is staged: it runs the binary this repository builds and prints
# whatever that prints.
#
#   ./scripts/demo.sh          press enter between beats, narrate at your pace
#   AUTO=6 ./scripts/demo.sh   hands off, six seconds a beat
#
# Beat 4 clones over the network. That is git, not the tool — `stranger` itself
# never opens a socket, which is the point beat 5 makes. Everything is written
# under /tmp/stranger-demo and Ctrl-C is safe at any point.
set -u

REPO=$(cd "$(dirname "$0")/.." && pwd)
WORK=/tmp/stranger-demo
ORIGIN=${ORIGIN:-https://github.com/keirsalterego/stranger.git}
AUTO="${AUTO:-}"

B=$'\e[1m'; DIM=$'\e[2m'; GRN=$'\e[32m'; OFF=$'\e[0m'

pause() {
  if [ -n "$AUTO" ]; then
    sleep "$AUTO"
  else
    printf '\n%s' "$DIM"
    read -r -p "── enter ──" _ </dev/tty
    printf '%s\r\033[K' "$OFF"
  fi
}

# Type it out first. A command that appears all at once reads as a paste, and
# the point of the video is that these are real.
say() {
  printf '\n%s$ %s' "$GRN" "$B"
  local i
  for ((i = 0; i < ${#1}; i++)); do
    printf '%s' "${1:i:1}"
    sleep 0.012
  done
  printf '%s\n' "$OFF"
  sleep 0.25
  eval "$1"
}

note() { printf '\n%s%s%s\n' "$DIM" "$1" "$OFF"; }

cd "$REPO" || exit 1
if [ ! -x target/release/stranger ]; then
  echo "target/release/stranger is not built; run \`make\` first" >&2
  exit 1
fi
rm -rf "$WORK"
mkdir -p "$WORK"
clear

note "1/12 — the empty manifest, first thing on screen"
say 'cat Cargo.toml'
pause

note "2/12 — one crate"
say 'cargo tree'
pause

note "3/12 — one [[package]] block: Cargo resolved nothing but us"
say "grep -c '^\[\[package\]\]' Cargo.lock"
pause

note "4/12 — straight from GitHub, landing on the freeze tag"
say "cd $WORK && git clone -q $ORIGIN s && cd s && git describe --tags"
pause

note "5/12 — empty CARGO_HOME, network off. No registry index has ever existed here."
say "CARGO_HOME=$WORK/empty CARGO_NET_OFFLINE=true cargo build --release --locked --offline"
pause

note "6/12 — the poisoned fixture. Read the last line out loud."
say './target/release/stranger scan fixtures/poisoned.package-lock.json'
pause

note "7/12 — the CI gate. Two files, two exit codes, nothing on screen."
say './target/release/stranger scan fixtures/npm-xs.package-lock.json --fail-on high -q >/dev/null; echo "clean tree -> exit $?"'
say './target/release/stranger scan fixtures/poisoned.package-lock.json --fail-on high -q >/dev/null; echo "poisoned   -> exit $?"'
pause

note "8/12 — clause 3 on screen: in-degree 0, nothing depends on it"
say './target/release/stranger tree lodahs fixtures/poisoned.package-lock.json'
pause

note "9/12 — same file, same reader, the other answer"
say './target/release/stranger tree accepts fixtures/poisoned.package-lock.json --depth 2'
pause

note "10/12 — the ablation. 36 to 1 at 90% coverage, and recall stays at 1.000."
say "sed -n '/^| corpus kept | clause 3/,/^\$/p' $REPO/README.md"
pause

note "11/12 — two directories, deliberately different lengths, one binary"
say "cd $REPO && make repro"
pause

note "12/12 — serde_json: 1.2 billion downloads, and the case behind the claim"
say "sed -n '/^## Package Killer/,/1,997,016 agreed/p' README.md"

printf '\n%s%sEvery number in this video regenerates from a make target.%s\n\n' "$B" "$GRN" "$OFF"
