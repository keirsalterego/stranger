#!/usr/bin/env sh
# The differential campaign from tests/json_conformance.rs: this repository's
# JSON parser against CPython's, on generated documents and mutations of them.
#
# Dev-time tooling. `python3` is not a dependency of `stranger` — it is not in
# Cargo.toml, it never runs inside the binary, and `cargo test` on a machine
# without it runs the whole rest of the suite. The test itself says so and
# returns quietly rather than failing when Python is absent.
#
#   ./scripts/json-differential.sh
#   STRANGER_JSON_CAMPAIGN=1000000 ./scripts/json-differential.sh
set -eu
cd "$(dirname "$0")/.."

# Release, because the campaign is parser-bound at both ends and the debug
# build spends its time in bounds checks rather than in the grammar.
cargo test --release --test json_conformance -- \
  --ignored --exact --nocapture differential_against_python
