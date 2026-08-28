.PHONY: all test bench proof clean fmt lint

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
