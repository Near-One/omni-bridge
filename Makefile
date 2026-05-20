.PHONY: rust-lint rust-lint-near

MAKEFILE_DIR :=  $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

OUT_DIR ?= $(MAKEFILE_DIR)/near/target/near

LINT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions

NEAR_MANIFEST := $(MAKEFILE_DIR)/near/Cargo.toml
OMNI_BRIDGE_MANIFEST := $(MAKEFILE_DIR)/near/omni-bridge/Cargo.toml
OMNI_TOKEN_MANIFEST := $(MAKEFILE_DIR)/near/omni-token/Cargo.toml
TOKEN_DEPLOYER := $(MAKEFILE_DIR)/near/token-deployer/Cargo.toml
EVM_PROVER_MANIFEST := $(MAKEFILE_DIR)/near/omni-prover/evm-prover/Cargo.toml
WORMHOLE_OMNI_PROVER_PROXY_MANIFEST := $(MAKEFILE_DIR)/near/omni-prover/wormhole-omni-prover-proxy/Cargo.toml
MPC_OMNI_PROVER_MANIFEST := $(MAKEFILE_DIR)/near/omni-prover/mpc-omni-prover/Cargo.toml
MOCK_PROVER_MANIFEST := $(MAKEFILE_DIR)/near/mock/mock-prover/Cargo.toml
MOCK_TOKEN_MANIFEST := $(MAKEFILE_DIR)/near/mock/mock-token/Cargo.toml

# CHAIN_ID byte embedded in outgoing Wormhole payloads. Must match ChainKind on the NEAR side:
# 2 = Sol, 12 = Fogo. See solana/programs/bridge_token_factory/build.rs.
SOL_CHAIN_ID  := 2
FOGO_CHAIN_ID := 12

# Provided by the operator/CI for FOGO builds and deploys.
FOGO_PROGRAM_ID ?=
FOGO_RPC_URL ?=
# Path (relative to solana/) of the keypair whose address is the SVM program ID
# (used for Solana and FOGO builds/deploys). For FOGO it must resolve to the same
# address that was baked into the .so via FOGO_PROGRAM_ID at build time.
SVM_PROGRAM_KEYPAIR ?= target/deploy/bridge_token_factory-keypair.json

clippy: clippy-near

clippy-near:
	cargo clippy --manifest-path $(NEAR_MANIFEST) --all-features -- $(LINT_OPTIONS)

fmt-near:
	cargo fmt --all --check --manifest-path $(NEAR_MANIFEST)

rust-build-omni-bridge:
	cargo near build reproducible-wasm --manifest-path $(OMNI_BRIDGE_MANIFEST) --out-dir $(OUT_DIR)

rust-build-omni-token:
	cargo near build reproducible-wasm --manifest-path $(OMNI_TOKEN_MANIFEST) --out-dir $(OUT_DIR)

rust-build-token-deployer:
	cargo near build reproducible-wasm --manifest-path $(TOKEN_DEPLOYER) --out-dir $(OUT_DIR)

rust-build-evm-prover:
	cargo near build reproducible-wasm --manifest-path $(EVM_PROVER_MANIFEST) --out-dir $(OUT_DIR)

rust-build-wormhole-omni-prover-proxy:
	cargo near build reproducible-wasm --manifest-path $(WORMHOLE_OMNI_PROVER_PROXY_MANIFEST) --out-dir $(OUT_DIR)

rust-build-mpc-omni-prover:
	cargo near build reproducible-wasm --manifest-path $(MPC_OMNI_PROVER_MANIFEST) --out-dir $(OUT_DIR)

rust-build-mock-prover:
	cargo near build reproducible-wasm --manifest-path $(MOCK_PROVER_MANIFEST) --out-dir $(OUT_DIR)

rust-build-mock-token:
	cargo near build reproducible-wasm --manifest-path $(MOCK_TOKEN_MANIFEST) --out-dir $(OUT_DIR)

rust-build-near: rust-build-omni-bridge rust-build-omni-token rust-build-token-deployer rust-build-evm-prover rust-build-wormhole-omni-prover-proxy rust-build-mpc-omni-prover rust-build-mock-prover rust-build-mock-token

solana-generate-program-id:
	cd solana && solana-keygen new -o $(SVM_PROGRAM_KEYPAIR) --no-passphrase

solana-build-dev: ENV = devnet
solana-build: ENV = mainnet
solana-build-dev solana-build:
	cd solana && \
	PROGRAM_ID=$$(solana address -k $(SVM_PROGRAM_KEYPAIR)) && \
	RUSTUP_TOOLCHAIN="nightly-2026-01-08" anchor build --verifiable --program-name bridge_token_factory --env "PROGRAM_ID=$$PROGRAM_ID" --env "CHAIN_ID=$(SOL_CHAIN_ID)" -- --no-default-features --features $(ENV)

# FOGO is an SVM chain that runs the same Solana program but encodes a different
# ChainKind byte in payloads. Uses the wormhole-anchor-sdk `mainnet` feature on the
# assumption that the Wormhole core bridge and Post Message Shim are at the same
# addresses on FOGO as on Solana mainnet. If FOGO uses a different Wormhole core
# address, a `fogo` feature must be added in Near-One/wormhole-scaffolding.
solana-build-fogo-dev: ENV = devnet
solana-build-fogo:     ENV = mainnet
solana-build-fogo-dev solana-build-fogo:
	@test -n "$(FOGO_PROGRAM_ID)" || { echo "FOGO_PROGRAM_ID is required (e.g., make solana-build-fogo FOGO_PROGRAM_ID=<addr>)" >&2; exit 1; }
	cd solana && \
	RUSTUP_TOOLCHAIN="nightly-2026-01-08" anchor build --verifiable --program-name bridge_token_factory --env "PROGRAM_ID=$(FOGO_PROGRAM_ID)" --env "CHAIN_ID=$(FOGO_CHAIN_ID)" -- --no-default-features --features $(ENV) && \
	mv target/verifiable/bridge_token_factory.so target/verifiable/bridge_token_factory_fogo.so && \
	mv target/idl/bridge_token_factory.json target/idl/bridge_token_factory_fogo.json

solana-build-ci:
	cd solana && \
	export PROGRAM_ID=dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe && \
	RUSTUP_TOOLCHAIN="nightly-2026-01-08" anchor build --verifiable --program-name bridge_token_factory --env "PROGRAM_ID=$$PROGRAM_ID" --env "CHAIN_ID=$(SOL_CHAIN_ID)" -- --no-default-features --features mainnet

solana-build-ci-fogo:
	@test -n "$(FOGO_PROGRAM_ID)" || { echo "FOGO_PROGRAM_ID is required (set as a CI variable)" >&2; exit 1; }
	cd solana && \
	RUSTUP_TOOLCHAIN="nightly-2026-01-08" anchor build --verifiable --program-name bridge_token_factory --env "PROGRAM_ID=$(FOGO_PROGRAM_ID)" --env "CHAIN_ID=$(FOGO_CHAIN_ID)" -- --no-default-features --features mainnet && \
	mv target/verifiable/bridge_token_factory.so target/verifiable/bridge_token_factory_fogo.so && \
	mv target/idl/bridge_token_factory.json target/idl/bridge_token_factory_fogo.json

solana-deploy-dev: ENV = devnet
solana-deploy: ENV = mainnet
solana-deploy-dev solana-deploy:
	cd solana && \
	RUSTUP_TOOLCHAIN="nightly-2026-01-08" \
	anchor deploy --verifiable --program-name bridge_token_factory --provider.cluster $(ENV)

# Anchor's --provider.cluster shortcut names (mainnet/devnet/testnet/localnet) do not
# resolve to FOGO; we pass an explicit RPC URL instead.
# `anchor deploy --verifiable` reads target/verifiable/bridge_token_factory.so, so we
# restore the canonical name from the `_fogo` artifact produced by solana-build-fogo*.
solana-deploy-fogo:
	@test -n "$(FOGO_RPC_URL)" || { echo "FOGO_RPC_URL is required (e.g., make solana-deploy-fogo FOGO_RPC_URL=https://...)" >&2; exit 1; }
	@test -f solana/target/verifiable/bridge_token_factory_fogo.so || { echo "solana/target/verifiable/bridge_token_factory_fogo.so not found — run \`make solana-build-fogo-dev\` first" >&2; exit 1; }
	cd solana && \
	cp target/verifiable/bridge_token_factory_fogo.so target/verifiable/bridge_token_factory.so && \
	cp target/idl/bridge_token_factory_fogo.json target/idl/bridge_token_factory.json && \
	RUSTUP_TOOLCHAIN="nightly-2026-01-08" \
	anchor deploy --verifiable \
		--program-name bridge_token_factory \
		--program-keypair $(SVM_PROGRAM_KEYPAIR) \
		--provider.cluster $(FOGO_RPC_URL)

rust-run-tests:
	cargo nextest run --manifest-path $(NEAR_MANIFEST) --all-features

solana-run-tests:
	cd $(MAKEFILE_DIR)/solana && cargo build-sbf
	cd $(MAKEFILE_DIR)/solana && cargo test --package bridge_token_factory --test mollusk --features no-entrypoint
