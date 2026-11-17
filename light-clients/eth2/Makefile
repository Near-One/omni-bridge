MAKEFILE_DIR :=  $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

MANIFEST := $(MAKEFILE_DIR)/Cargo.toml
ETH2_CLIENT_MANIFEST := $(MAKEFILE_DIR)/eth2-client/Cargo.toml

build: build-eth2-client

build-eth2-client:
	cargo near build reproducible-wasm --manifest-path $(ETH2_CLIENT_MANIFEST)

test:
	cd $(MAKEFILE_DIR)/eth2-client && ./test.sh

fmt:
	cargo fmt --all --check --manifest-path $(MANIFEST)

clippy:
	cargo clippy --manifest-path $(MANIFEST)

check: clippy fmt
