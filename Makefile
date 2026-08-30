.PHONY: all test bench proof clean fmt lint ablation repro sweep docs

all:
	cargo build --release --locked

test:
	cargo test

bench:
	@./scripts/bench.sh

proof:
	@./scripts/proof.sh

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets -- -D warnings
	cargo fmt --check

clean:
	cargo clean

ablation:
	cargo test --release --test ablation -- --nocapture --include-ignored

repro:
	@./scripts/repro.sh

# Every lockfile on the machine this runs on, through the reader for it. Not
# part of `make test` and cannot be: the corpus is whatever happens to be on
# the disk, so it is neither fixed nor portable. That is also the point — it
# asks whether the readers handle the ordinary file, which the fixtures cannot,
# and it is what found the four reader bugs fixed on 2026-08-30.
sweep:
	@./scripts/sweep.sh

# The one target that needs something this repository does not ship. `mdbook` is
# a documentation tool, not a dependency of the binary — `make`, `make test`,
# `make bench`, `make proof` and `make repro` all run with nothing but a Rust
# toolchain — but a target that dies with `command not found` is a bad first
# impression, so it says which of the three steps it skipped and why.
#
# check-links and check-output need no mdbook and run regardless: a rotted link
# or a console block the tool no longer prints is worth catching on a machine
# that cannot build the book.
docs:
	@command -v mdbook >/dev/null 2>&1 && mdbook build docs 	  || echo "mdbook not installed — skipping the book build; the two checks below still run."
	@./docs/check-links.py
	@./docs/check-output.py
