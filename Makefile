.PHONY: all test bench proof clean fmt lint ablation repro docs

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
