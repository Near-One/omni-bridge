.PHONY: rust-lint rust-lint-near rust-lint-omni-relayer

LINT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions
RUSTFLAGS = -C link-arg=-s

NEAR_MANIFEST = ./near/Cargo.toml
OMNI_RELAYER_MANIFEST = ./omni-relayer/Cargo.toml

clippy: clippy-near clippy-omni-relayer

clippy-near: rust-build-token
	cargo clippy --manifest-path $(NEAR_MANIFEST) -- $(LINT_OPTIONS)

fmt-near:
	cargo fmt --all --check --manifest-path $(NEAR_MANIFEST)

fmt-omni-relayer:
	cargo fmt --all --check --manifest-path $(OMNI_RELAYER_MANIFEST)

clippy-omni-relayer:
	cargo clippy --manifest-path $(OMNI_RELAYER_MANIFEST) -- $(LINT_OPTIONS)

rust-build-token:
	RUSTFLAGS='$(RUSTFLAGS)' cargo build --target wasm32-unknown-unknown --release --manifest-path $(NEAR_MANIFEST) --package omni-token

rust-build-near: rust-build-token
	RUSTFLAGS='$(RUSTFLAGS)' cargo build --target wasm32-unknown-unknown --release --manifest-path $(NEAR_MANIFEST)

test-near: rust-build-near
	cargo nextest run --manifest-path $(NEAR_MANIFEST)
