.PHONY: rust-lint rust-lint-near rust-lint-omni-relayer

LINT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions
RUSTFLAGS = -C link-arg=-s

NEAR_MANIFEST = ./near/Cargo.toml
OMNI_RELAYER_MANIFEST = ./omni-relayer/Cargo.toml

rust-lint: rust-lint-near #rust-lint-relayer

rust-lint-near:
	cargo clippy --manifest-path $(NEAR_MANIFEST) -- $(LINT_OPTIONS)

rust-lint-omni-relayer:
	cargo clippy --manifest-path $(OMNI_RELAYER_MANIFEST) -- $(LINT_OPTIONS)

rust-build-near:
	RUSTFLAGS='$(RUSTFLAGS)' cargo build --target wasm32-unknown-unknown --release --manifest-path $(NEAR_MANIFEST)
