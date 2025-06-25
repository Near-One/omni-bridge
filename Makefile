.PHONY: rust-lint rust-lint-near rust-lint-omni-relayer

MAKEFILE_DIR :=  $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

OUT_DIR ?= $(MAKEFILE_DIR)/near/target/near

LINT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions

NEAR_MANIFEST := $(MAKEFILE_DIR)/near/Cargo.toml
OMNI_BRIDGE_MANIFEST := $(MAKEFILE_DIR)/near/omni-bridge/Cargo.toml
OMNI_TOKEN_MANIFEST := $(MAKEFILE_DIR)/near/omni-token/Cargo.toml
TOKEN_DEPLOYER := $(MAKEFILE_DIR)/near/token-deployer/Cargo.toml
OMNI_PROVER_MANIFEST := $(MAKEFILE_DIR)/near/omni-prover/omni-prover/Cargo.toml
EVM_PROVER_MANIFEST := $(MAKEFILE_DIR)/near/omni-prover/evm-prover/Cargo.toml
BTC_PROVER_MANIFEST := $(MAKEFILE_DIR)/near/omni-prover/btc-prover/Cargo.toml
WORMHOLE_OMNI_PROVER_PROXY_MANIFEST := $(MAKEFILE_DIR)/near/omni-prover/wormhole-omni-prover-proxy/Cargo.toml
MOCK_PROVER_MANIFEST := $(MAKEFILE_DIR)/near/mock/mock-prover/Cargo.toml
MOCK_TOKEN_MANIFEST := $(MAKEFILE_DIR)/near/mock/mock-token/Cargo.toml

OMNI_RELAYER_MANIFEST := $(MAKEFILE_DIR)/omni-relayer/Cargo.toml

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
	cargo near build reproducible-wasm --manifest-path $(OMNI_BRIDGE_MANIFEST) --out-dir $(OUT_DIR)

rust-build-omni-token:
	cargo near build reproducible-wasm --manifest-path $(OMNI_TOKEN_MANIFEST) --out-dir $(OUT_DIR)

rust-build-token-deployer:
	cargo near build reproducible-wasm --manifest-path $(TOKEN_DEPLOYER) --out-dir $(OUT_DIR)

rust-build-omni-prover:
	cargo near build reproducible-wasm --manifest-path $(OMNI_PROVER_MANIFEST) --out-dir $(OUT_DIR)

rust-build-evm-prover:
	cargo near build reproducible-wasm --manifest-path $(EVM_PROVER_MANIFEST) --out-dir $(OUT_DIR)

rust-build-wormhole-omni-prover-proxy:
	cargo near build reproducible-wasm --manifest-path $(WORMHOLE_OMNI_PROVER_PROXY_MANIFEST) --out-dir $(OUT_DIR)

rust-build-mock-prover:
	cargo near build reproducible-wasm --manifest-path $(MOCK_PROVER_MANIFEST) --out-dir $(OUT_DIR)

rust-build-mock-token:
	cargo near build reproducible-wasm --manifest-path $(MOCK_TOKEN_MANIFEST) --out-dir $(OUT_DIR)

rust-build-near: rust-build-omni-bridge rust-build-omni-token rust-build-token-deployer rust-build-omni-prover rust-build-evm-prover rust-build-wormhole-omni-prover-proxy rust-build-mock-prover rust-build-mock-token

solana-generate-program-id:
	solana-keygen new -o solana/bridge_token_factory/target/deploy/bridge_token_factory-keypair.json --no-passphrase

solana-build-dev: ENV = devnet
solana-build: ENV = mainnet
solana-build-dev solana-build:
	cd solana/bridge_token_factory && \
	PROGRAM_ID=$$(solana address -k target/deploy/bridge_token_factory-keypair.json) && \
	RUSTUP_TOOLCHAIN="nightly-2024-11-19" anchor build --verifiable --env "PROGRAM_ID=$$PROGRAM_ID" -- --no-default-features --features $(ENV)

solana-build-ci:
	cd solana/bridge_token_factory && \
	export PROGRAM_ID=dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe && \
	RUSTUP_TOOLCHAIN="nightly-2024-11-19" anchor build --verifiable --env "PROGRAM_ID=$$PROGRAM_ID" -- --no-default-features --features mainnet

solana-deploy-dev: ENV = devnet
solana-deploy: ENV = mainnet
solana-deploy-dev solana-deploy:
	cd solana/bridge_token_factory && \
	RUSTUP_TOOLCHAIN="nightly-2024-11-19" \
	anchor deploy --verifiable --program-name bridge_token_factory --provider.cluster $(ENV)

rust-run-tests:
	cargo nextest run --manifest-path $(NEAR_MANIFEST)
