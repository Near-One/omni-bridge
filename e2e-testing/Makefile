
##########################################################
# Makefile prologue
##########################################################

MAKEFLAGS += --no-builtin-rules --no-builtin-variables --warn-undefined-variables
unexport MAKEFLAGS
.DELETE_ON_ERROR:
.SUFFIXES:
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c

#####################################################

TESTING_ROOT := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

TIMESTAMP := $(shell date -u +%Y%m%d-%H%M%S)

EVM_DIR := $(TESTING_ROOT)/../evm
EVM_SCRIPT_DIR := $(TESTING_ROOT)/evm_scripts
ETHEREUM_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/ethereum_deploy_results

NEAR_DIR := $(TESTING_ROOT)/../near
NEAR_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/near_deploy_results
NEAR_INIT_PARAMS_FILE := $(TESTING_ROOT)/near_init_params.json

SOLANA_DIR := $(TESTING_ROOT)/../solana
SOLANA_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/solana_deploy_results

NEAR_BRIDGE_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni_bridge.json


##########################################################
# EVM
##########################################################

.PHONY: evm-build evm-scripts-build

EVM_COMPILE_STAMP := $(TESTING_ROOT)/.evm-compile.stamp
EVM_ARTIFACTS_DIR := $(TESTING_ROOT)/evm_artifacts
EVM_SCRIPT_COMPILE_STAMP := $(TESTING_ROOT)/.evm-scripts-compile.stamp

EVM_NETWORKS := sepolia arbitrumSepolia baseSepolia

EVM_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/evm_deploy_results

DEPLOY_EVM_TOKEN_IMPL = yarn --silent --cwd $(EVM_DIR) hardhat deploy-token-impl --network $(1)
DEPLOY_EVM_OMNI_BRIDGE_CONTRACT = yarn --silent --cwd $(EVM_DIR) hardhat deploy-bridge-token-factory --network $(1) --bridge-token-impl $(2) --near-bridge-account-id $(3)
DEPLOY_EVM_FAKE_PROVER = yarn --silent --cwd $(EVM_DIR) hardhat deploy-fake-prover --network $(1)
DEPLOY_EVM_ENEAR_PROXY = yarn --silent --cwd $(EVM_DIR) hardhat deploy-e-near-proxy --network $(1) --enear $(2)

DEPLOY_EVM_BYTECODE = yarn --silent --cwd $(EVM_SCRIPT_DIR) hardhat deploy-bytecode --network $(1) --bytecode $(2)
DEPLOY_EVM_TEST_TOKEN = yarn --silent --cwd $(EVM_SCRIPT_DIR) hardhat deploy-test-token --network $(1) --name $(2) --symbol $(3)

ENEAR_CREATION_TEMPLATE_FILE := $(TESTING_ROOT)/bin/eNear_creation.template

$(EVM_DEPLOY_RESULTS_DIR):
	mkdir -p $@

evm-build: $(EVM_COMPILE_STAMP)
$(EVM_COMPILE_STAMP):
	@echo "Compiling EVM contracts"
	mkdir -p $(EVM_ARTIFACTS_DIR) && \
	yarn --cwd $(EVM_DIR) install --frozen-lockfile && \
	yarn --cwd $(EVM_DIR) hardhat compile && \
	cp -r $(EVM_DIR)/build/* $(EVM_ARTIFACTS_DIR)
	touch $@

evm-scripts-build: $(EVM_SCRIPT_COMPILE_STAMP)
$(EVM_SCRIPT_COMPILE_STAMP):
	@echo "Compiling EVM scripts"
	yarn --cwd $(EVM_SCRIPT_DIR) install && \
	yarn --cwd $(EVM_SCRIPT_DIR) hardhat compile
	touch $@

# Arguments:
# 		$(1) -  the network name
define generate_evm_deploy_rules

.PHONY: $(1)-deploy-fake-prover $(1)-deploy-enear $(1)-deploy-enear-proxy $(1)-deploy-bridge $(1)-deploy-token-impl $(1)-deploy-test-token

$(1)_DEPLOY_RESULTS_DIR := $(EVM_DEPLOY_RESULTS_DIR)/$(1)

$$($(1)_DEPLOY_RESULTS_DIR): | $(EVM_DEPLOY_RESULTS_DIR)
	mkdir -p $$@

$(1)-deploy: $(1)-deploy-bridge $(1)-deploy-enear-proxy $(1)-deploy-test-token

$(1)_BRIDGE_CONTRACT_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/omni_bridge.json
$(1)_TOKEN_IMPL_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/token_factory.json
$(1)_FAKE_PROVER_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/fake_prover.json

$(1)_ENEAR_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/eNear.json
$(1)_ENEAR_PROXY_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/eNearProxy.json
$(1)_ENEAR_CREATION_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/eNear_creation

$(1)_TEST_TOKEN_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/test_token.json

$(1)-deploy-fake-prover: $$($(1)_FAKE_PROVER_ADDRESS_FILE)
$$($(1)_FAKE_PROVER_ADDRESS_FILE): $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_FAKE_PROVER,$(1)) 2>/dev/stderr 1> $$@

$(1)-deploy-enear: $$($(1)_ENEAR_ADDRESS_FILE)
$$($(1)_ENEAR_ADDRESS_FILE): $$($(1)_ENEAR_CREATION_FILE) $(EVM_SCRIPT_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_BYTECODE,$(1),$$($(1)_ENEAR_CREATION_FILE)) 2>/dev/stderr 1> $$@

$(1)-deploy-enear-proxy: $$($(1)_ENEAR_PROXY_ADDRESS_FILE)
$$($(1)_ENEAR_PROXY_ADDRESS_FILE): $$($(1)_ENEAR_ADDRESS_FILE) $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_ENEAR_PROXY,$(1),$$(shell cat $$($(1)_ENEAR_ADDRESS_FILE) | jq -r .contractAddress)) 2>/dev/stderr 1> $$@

$(1)-deploy-bridge: $$($(1)_BRIDGE_CONTRACT_ADDRESS_FILE)
$$($(1)_BRIDGE_CONTRACT_ADDRESS_FILE): $$($(1)_TOKEN_IMPL_ADDRESS_FILE) $(NEAR_BRIDGE_ID_FILE) $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_OMNI_BRIDGE_CONTRACT,$(1),$$(shell cat $$($(1)_TOKEN_IMPL_ADDRESS_FILE) | jq -r .tokenImplAddress),$$(shell cat $(NEAR_BRIDGE_ID_FILE) | jq -r .contract_id)) 2>/dev/stderr 1> $$@

$(1)-deploy-token-impl: $$($(1)_TOKEN_IMPL_ADDRESS_FILE)
$$($(1)_TOKEN_IMPL_ADDRESS_FILE): $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_TOKEN_IMPL,$(1)) 2>/dev/stderr 1> $$@

$$($(1)_ENEAR_CREATION_FILE): $(ENEAR_CREATION_TEMPLATE_FILE) $$($(1)_FAKE_PROVER_ADDRESS_FILE) | $$($(1)_DEPLOY_RESULTS_DIR)
	cat $$< | \
	sed "s/<PROVER_ADDRESS>/$$(shell cat $$($(1)_FAKE_PROVER_ADDRESS_FILE) | jq -r .fakeProverAddress | sed 's/^0x//')/" > $$@


$(1)-deploy-test-token: $$($(1)_TEST_TOKEN_ADDRESS_FILE)
$$($(1)_TEST_TOKEN_ADDRESS_FILE): $(EVM_SCRIPT_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_TEST_TOKEN,$(1),E2ETestToken-$(TIMESTAMP),E2ETT-$(TIMESTAMP)) 2>/dev/stderr 1> $$@

endef

$(foreach network,$(EVM_NETWORKS),$(eval $(call generate_evm_deploy_rules,$(network))))

##########################################################
# NEAR
##########################################################

NEAR_BINARY_DIR := $(TESTING_ROOT)/near_artifacts

.PHONY: near-build near-deploy

# List all expected WASM binaries
NEAR_BINARIES := evm_prover.wasm omni_bridge.wasm omni_prover.wasm token_deployer.wasm wormhole_omni_prover_proxy.wasm mock_token.wasm
NEAR_BINARY_PATHS := $(addprefix $(NEAR_BINARY_DIR)/,$(NEAR_BINARIES))

NEAR_BUILD_STAMP := $(TESTING_ROOT)/.near-build.stamp

DEPLOY_RESULTS := $(patsubst $(NEAR_BINARY_DIR)/%.wasm,$(NEAR_DEPLOY_RESULTS_DIR)/%.json,$(NEAR_BINARY_PATHS))

near-deploy: $(DEPLOY_RESULTS)

$(NEAR_DEPLOY_RESULTS_DIR):
	mkdir -p $@

near-build: $(NEAR_BUILD_STAMP)
$(NEAR_BUILD_STAMP):
	$(NEAR_DIR)/build.sh --output-dir $(NEAR_BINARY_DIR)
	touch $@

# Arguments:
# 		$(1) -  the path to the binary file
define generate_near_deploy_rules

$(1)_NAME := $$(basename $$(notdir $(1)))

$(NEAR_DEPLOY_RESULTS_DIR)/$$($(1)_NAME).json: $(NEAR_INIT_PARAMS_FILE) $(1) | $(NEAR_DEPLOY_RESULTS_DIR)
	INIT_FUNCTION=$$$$(jq -r '.$$($(1)_NAME).init_function // ""' $$<) && \
	INIT_ARGS=$$$$(jq -c '.$$($(1)_NAME).init_args // "{}"' $$<) && \
	./scripts/deploy-near-contract.sh $(1) $$@ $$($(1)_NAME)-$(TIMESTAMP).testnet "$$$$INIT_FUNCTION" "$$$$INIT_ARGS"

$(1): $(NEAR_BUILD_STAMP)

endef

$(foreach binary,$(NEAR_BINARY_PATHS),$(eval $(call generate_near_deploy_rules,$(binary))))


########################################
# Solana
########################################

SOLANA_BUILD_STAMP := $(TESTING_ROOT)/.solana-build.stamp
SOLANA_ARTIFACTS_DIR := $(TESTING_ROOT)/solana_artifacts

SOLANA_PROGRAMS := bridge_token_factory
SOLANA_PROGRAMS_KEYPAIRS := $(foreach program,$(SOLANA_PROGRAMS),$(SOLANA_DIR)/$(program)/target/deploy/$(program)-keypair.json)
SOLANA_PROGRAMS_BINARIES := $(foreach program,$(SOLANA_PROGRAMS),$(SOLANA_ARTIFACTS_DIR)/$(program)/target/deploy/$(program).so)

solana-build: $(SOLANA_BUILD_STAMP)
$(SOLANA_BUILD_STAMP): $(SOLANA_PROGRAMS_KEYPAIRS) $(SOLANA_PROGRAMS_BINARIES)
	touch $@

# Arguments:
# 		$(1) -  the program name
define generate_solana_build_rules

$(SOLANA_DIR)/$(1)/target/deploy/$(1)-keypair.json: $(TESTING_ROOT)/$(1)-keypair.json
	mkdir -p $$(dir $$@) && \
	cp $$< $$@

$(SOLANA_ARTIFACTS_DIR)/$(1)/target/deploy/$(1).so: $(SOLANA_DIR)/$(1)/target/deploy/$(1)-keypair.json
	mkdir -p $(SOLANA_ARTIFACTS_DIR)/$(1) && \
	cd $(SOLANA_DIR)/$(1) && \
	anchor build && \
	cp -r $(SOLANA_DIR)/$(1)/target/* $(SOLANA_ARTIFACTS_DIR)/$(1)

endef 

$(foreach program,$(SOLANA_PROGRAMS),$(eval $(call generate_solana_build_rules,$(program))))

