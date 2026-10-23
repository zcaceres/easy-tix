.PHONY: build test

build:
	cargo build --release --locked --offline

test:
	cargo test --release --locked --offline
