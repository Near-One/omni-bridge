.PHONY: build test declare-token declare-bridge deploy-bridge

MAKEFILE_DIR := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))
NETWORK      ?= sepolia

OMNI_BRIDGE_CHAIN_ID := 10
STRK_TOKEN_ADDRESS   := 0x04718f5a0Fc34cC1AF16A1cdee98fFB20C31f5cD61D6Ab07201858f4287c938D

# Per-network constants
ifeq ($(NETWORK),sepolia)
  OMNI_BRIDGE_DERIVED_ADDRESS := 0xc31351cbafc2ad612e6dc3e27e1e66615cc8aa73
  DEFAULT_ADMIN               := 0x0733aFFf0d2dA821cb24FA41df33a506D570325B8C15dC2778dBD9Bedb1a22FB
else ifeq ($(NETWORK),mainnet)
  OMNI_BRIDGE_DERIVED_ADDRESS := 0x22eb4d37677ed931d9de2218cece1a832a147490
  DEFAULT_ADMIN               :=
else
  $(error Unknown network: $(NETWORK). Valid options: sepolia, mainnet)
endif

BRIDGE_TOKEN_CLASS_HASH_FILE := $(MAKEFILE_DIR)/target/$(NETWORK)_bridge_token.classhash


build:
	scarb build

test:
	scarb test

declare-token:
	@mkdir -p $(MAKEFILE_DIR)/target
	sncast declare \
		--contract-name BridgeToken \
		--network $(NETWORK) \
		2>&1 | tee /dev/stderr \
		| grep -oE 'class.hash[: ]+0x[0-9a-fA-F]+' | grep -oE '0x[0-9a-fA-F]+' \
		> $(BRIDGE_TOKEN_CLASS_HASH_FILE)
	@echo "BridgeToken class hash saved to $(BRIDGE_TOKEN_CLASS_HASH_FILE)"

declare-bridge:
	sncast declare \
		--contract-name OmniBridge \
		--network $(NETWORK)

deploy-bridge:
	@class_hash=$$(cat $(BRIDGE_TOKEN_CLASS_HASH_FILE)); \
	set -x; \
	sncast deploy \
		--contract-name OmniBridge \
		--network $(NETWORK) \
		--arguments "$(OMNI_BRIDGE_DERIVED_ADDRESS), $(OMNI_BRIDGE_CHAIN_ID), $$class_hash, $(DEFAULT_ADMIN), $(STRK_TOKEN_ADDRESS)"
