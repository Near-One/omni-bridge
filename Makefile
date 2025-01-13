.PHONY: rust-lint rust-lint-near rust-lint-omni-relayer

LINT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions

NEAR_MANIFEST = ./near/Cargo.toml
OMNI_BRIDGE_MANIFEST = ./near/omni-bridge/Cargo.toml
OMNI_TOKEN_MANIFEST = ./near/omni-token/Cargo.toml
TOKEN_DEPLOYER = ./near/token-deployer/Cargo.toml
MOCK_PROVER_MANIFEST = ./near/mock/mock-prover/Cargo.toml
MOCK_TOKEN_MANIFEST = ./near/mock/mock-token/Cargo.toml

OMNI_RELAYER_MANIFEST = ./omni-relayer/Cargo.toml

clippy: clippy-near clippy-omni-relayer

clippy-near:
	cargo clippy --manifest-path $(NEAR_MANIFEST) -- $(LINT_OPTIONS)

fmt-near:
	cargo fmt --all --check --manifest-path $(NEAR_MANIFEST)

fmt-omni-relayer:
	cargo fmt --all --check --manifest-path $(OMNI_RELAYER_MANIFEST)

clippy-omni-relayer:
	cargo clippy --manifest-path $(OMNI_RELAYER_MANIFEST) -- $(LINT_OPTIONS)

rust-build-omni-bridge:
	cargo near build non-reproducible-wasm --manifest-path $(OMNI_BRIDGE_MANIFEST) --no-abi
	
rust-build-omni-token:
	cargo near build non-reproducible-wasm --manifest-path $(OMNI_TOKEN_MANIFEST) --no-abi
	
rust-build-token-deployer:
	cargo near build non-reproducible-wasm --manifest-path $(TOKEN_DEPLOYER) --no-abi
	
rust-build-mock-prover:
	cargo near build non-reproducible-wasm --manifest-path $(MOCK_PROVER_MANIFEST) --no-abi
	
rust-build-mock-token:
	cargo near build non-reproducible-wasm --manifest-path $(MOCK_TOKEN_MANIFEST) --no-abi

rust-build-near: rust-build-omni-bridge rust-build-omni-token rust-build-token-deployer rust-build-mock-prover rust-build-mock-token

rust-build-tests:
	cargo build --manifest-path $(NEAR_MANIFEST) --tests --all-features

rust-run-tests: rust-build-tests
	cargo nextest run --manifest-path $(NEAR_MANIFEST)
