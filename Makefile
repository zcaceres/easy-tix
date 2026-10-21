.PHONY: build test test-bdd

build:
	cargo build --release --locked --offline

test: build
	cargo test --locked --offline
	python3 -m unittest discover -s tests -v

test-bdd: build
	TICKET_SCRIPT="$(CURDIR)/target/release/ticket" python3 -m behave --format progress
