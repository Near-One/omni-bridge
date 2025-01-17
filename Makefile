.PHONY: rust-lint rust-lint-near rust-lint-omni-relayer

LINT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions

NEAR_MANIFEST = ./near/Cargo.toml
OMNI_BRIDGE_MANIFEST = ./near/omni-bridge/Cargo.toml
OMNI_TOKEN_MANIFEST = ./near/omni-token/Cargo.toml
TOKEN_DEPLOYER = ./near/token-deployer/Cargo.toml
OMNI_PROVER_MANIFEST = ./near/omni-prover/omni-prover/Cargo.toml
EVM_PROVER_MANIFEST = ./near/omni-prover/evm-prover/Cargo.toml
WORMHOLE_OMNI_PROVER_PROXY_MANIFEST = ./near/omni-prover/wormhole-omni-prover-proxy/Cargo.toml
MOCK_PROVER_MANIFEST = ./near/omni-prover/mock/mock-prover/Cargo.toml
MOCK_TOKEN_MANIFEST = ./near/omni-prover/mock/mock-token/Cargo.toml

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

rust-build-omni-bridge: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(OMNI_BRIDGE_MANIFEST)
	cp $(TARGET_WASM_DIR)/omni_bridge/omni_bridge.wasm $(RES_DIR)/omni_bridge.wasm
	
rust-build-omni-token: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(OMNI_TOKEN_MANIFEST)
	cp $(TARGET_WASM_DIR)/omni_token/omni_token.wasm $(RES_DIR)/omni_token.wasm
	
rust-build-token-deployer: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(TOKEN_DEPLOYER)
	cp $(TARGET_WASM_DIR)/token_deployer/token_deployer.wasm $(RES_DIR)/token_deployer.wasm
	
rust-build-omni-prover: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(OMNI_PROVER_MANIFEST)
	cp $(TARGET_WASM_DIR)/omni_prover/omni_prover.wasm $(RES_DIR)/omni_prover.wasm
	
rust-build-evm-prover: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(EVM_PROVER_MANIFEST)
	cp $(TARGET_WASM_DIR)/evm_prover/evm_prover.wasm $(RES_DIR)/evm_prover.wasm
	
rust-build-wormhole-omni-prover-proxy: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(WORMHOLE_OMNI_PROVER_PROXY_MANIFEST)
	cp $(TARGET_WASM_DIR)/wormhole_omni_prover_proxy/wormhole_omni_prover_proxy.wasm $(RES_DIR)/wormhole_omni_prover_proxy.wasm

rust-build-mock-prover: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(MOCK_PROVER_MANIFEST)
	cp $(TARGET_WASM_DIR)/mock_prover/mock_prover.wasm $(RES_DIR)/mock_prover.wasm

rust-build-mock-token: $(RES_DIR)
	cargo near build reproducible-wasm --manifest-path $(MOCK_TOKEN_MANIFEST)
	cp $(TARGET_WASM_DIR)/mock_token/mock_token.wasm $(RES_DIR)/mock_token.wasm

rust-build-near: rust-build-omni-bridge rust-build-omni-token rust-build-token-deployer rust-build-omni-prover rust-build-evm-prover rust-build-wormhole-omni-prover-proxy rust-build-mock-prover rust-build-mock-token

rust-build-tests:
	cargo build --manifest-path $(NEAR_MANIFEST) --tests --all-features

rust-run-tests: rust-build-tests
	cargo nextest run --manifest-path $(NEAR_MANIFEST)
