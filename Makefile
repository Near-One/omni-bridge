.PHONY: rust-lint rust-lint-near rust-lint-omni-relayer

LINT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions

NEAR_MANIFEST = ./near/Cargo.toml
OMNI_BRIDGE_MANIFEST = ./near/omni-bridge/Cargo.toml
OMNI_TOKEN_MANIFEST = ./near/omni-token/Cargo.toml
TOKEN_DEPLOYER = ./near/token-deployer/Cargo.toml
OMNI_PROVER_MANIFEST = ./near/omni-prover/omni-prover/Cargo.toml
EVM_PROVER_MANIFEST = ./near/omni-prover/evm-prover/Cargo.toml
WORMHOLE_OMNI_PROVER_PROXY_MANIFEST = ./near/omni-prover/wormhole-omni-prover-proxy/Cargo.toml
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
	cargo near build reproducible-wasm --manifest-path $(OMNI_BRIDGE_MANIFEST)
	
rust-build-omni-token:
	cargo near build reproducible-wasm --manifest-path $(OMNI_TOKEN_MANIFEST)
	
rust-build-token-deployer:
	cargo near build reproducible-wasm --manifest-path $(TOKEN_DEPLOYER)
	
rust-build-omni-prover:
	cargo near build reproducible-wasm --manifest-path $(OMNI_PROVER_MANIFEST)
	
rust-build-evm-prover:
	cargo near build reproducible-wasm --manifest-path $(EVM_PROVER_MANIFEST)
	
rust-build-wormhole-omni-prover-proxy:
	cargo near build reproducible-wasm --manifest-path $(WORMHOLE_OMNI_PROVER_PROXY_MANIFEST)

rust-build-mock-prover:
	cargo near build reproducible-wasm --manifest-path $(MOCK_PROVER_MANIFEST)

rust-build-mock-token:
	cargo near build reproducible-wasm --manifest-path $(MOCK_TOKEN_MANIFEST)

rust-build-near: $(RES_DIR) rust-build-omni-bridge rust-build-omni-token rust-build-token-deployer rust-build-omni-prover rust-build-evm-prover rust-build-wormhole-omni-prover-proxy rust-build-mock-prover rust-build-mock-token
	find $(TARGET_WASM_DIR) -name "*.wasm" -maxdepth 2 -exec cp -f {} $(RES_DIR)/ \;

rust-run-tests:
	cargo nextest run --manifest-path $(NEAR_MANIFEST)
