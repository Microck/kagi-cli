.PHONY: build test fmt lint check coverage setup-hooks

build:
	cargo build --release

test:
	cargo test -q --locked

fmt:
	cargo fmt --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

check: fmt lint test

coverage:
	rustup component add llvm-tools-preview
	cargo install cargo-llvm-cov --locked
	cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info

setup-hooks:
	git config core.hooksPath .githooks
