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

  echo '$ grep -q "forbid(unsafe_code)" src/lib.rs src/main.rs'
  # The attribute is the enforcement: the compiler rejects any unsafe beneath
  # it. Everything below is belt and braces.
  grep -q '^#!\[forbid(unsafe_code)\]' src/lib.rs && echo "src/lib.rs: forbidden"
  grep -q '^#!\[forbid(unsafe_code)\]' src/main.rs && echo "src/main.rs: forbidden"
  echo

  echo '$ grep -rnE "\\bunsafe\\s*(\\{|fn|impl|trait|extern)" src/'
  # Matching the bare word instead of the syntax fails on every comment that
  # explains why something avoids unsafe. That is a real ghost: it broke CI
  # once, on a comment in term.rs saying the isatty FFI would need one.
  if grep -rnE '\bunsafe\s*(\{|fn\b|impl\b|trait\b|extern\b)' src/; then
    echo "  ^ unexpected" >&2
    exit 1
  else
    echo "(no output — zero unsafe blocks, functions, impls or externs)"
  fi
  echo

  echo "release binary: $(ls -l target/release/stranger | awk '{print $5}') bytes"
  # Counted, not typed. This line said 140,066 for a day and a half, which is
  # the npm list on its own — there are three, and the number a reader would
  # check is the total.
  echo "  most of that is the $(cat corpus/*.txt | wc -l | tr -d ' ')-name corpus, compiled in with include_str!"
  echo "  so the tool needs no network and no cache directory at runtime."
} > "$OUT"

echo "wrote $OUT"
