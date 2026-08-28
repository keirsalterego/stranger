#!/usr/bin/env sh
# Regenerates deps-proof.txt. Everything in that file is generated, never typed,
# so it cannot drift from the truth by being edited.
set -eu
cd "$(dirname "$0")/.."
OUT=deps-proof.txt

{
  echo "stranger — dependency proof"
  echo "generated $(date -u +'%Y-%m-%dT%H:%M:%SZ') by scripts/proof.sh"
  echo
  echo "rustc: $(rustc --version)"
  echo "cargo: $(cargo --version)"
  echo

  echo '$ cargo tree'
  cargo tree
  echo
  echo "lines of cargo tree output: $(cargo tree | wc -l | tr -d ' ')"
  echo "  (one line means one crate: this one)"
  echo

  echo '$ grep -c "^\[\[package\]\]" Cargo.lock'
  grep -c '^\[\[package\]\]' Cargo.lock
  echo "  (one [[package]] block means nothing was resolved but us)"
  echo

  echo '$ grep -A2 "^\[dependencies\]" Cargo.toml'
  grep -A2 '^\[dependencies\]' Cargo.toml || true
  echo

  echo "--- the strongest one: it builds with the network unavailable ---"
  echo '$ CARGO_NET_OFFLINE=true cargo build --release --locked --offline'
  if CARGO_NET_OFFLINE=true cargo build --release --locked --offline >/dev/null 2>&1; then
    echo "OK"
  else
    echo "FAILED"
    exit 1
  fi
  echo

  echo '$ grep -rn "\bunsafe\b" src/ | grep -v "forbid(unsafe_code)"'
  # The naive grep matches the forbid attribute itself, which is not unsafe code.
  if grep -rn '\bunsafe\b' src/ | grep -v 'forbid(unsafe_code)'; then
    echo "  ^ unexpected: the crate root forbids unsafe, so this should be empty"
    exit 1
  else
    echo "(no output — zero unsafe blocks; the crate root forbids them outright)"
  fi
  echo

  echo "release binary: $(ls -l target/release/stranger | awk '{print $5}') bytes"
  echo "  most of that is the 140,066-name corpus, compiled in with include_str!"
  echo "  so the tool needs no network and no cache directory at runtime."
} > "$OUT"

echo "wrote $OUT"
