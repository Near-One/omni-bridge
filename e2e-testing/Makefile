.PHONY: ethereum-deploy evm-compile deploy-eth-bridge deploy-eth-token-impl create-near-bridge-id

TESTING_ROOT := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

EVM_DIR := $(TESTING_ROOT)/../evm
ETHEREUM_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/ethereum_deploy_results

NEAR_DIR := $(TESTING_ROOT)/../near
NEAR_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/near_deploy_results

NEAR_BRIDGE_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni_bridge.json

EVM_NETWORKS := sepolia arbitrumSepolia baseSepolia

##########################################################
# EVM
##########################################################

EVM_COMPILE_STAMP := $(TESTING_ROOT)/.evm-compile.stamp

EVM_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/evm_deploy_results

DEPLOY_EVM_TOKEN_IMPL = yarn --silent --cwd $(EVM_DIR) hardhat deploy-token-impl --network $(1)
DEPLOY_EVM_OMNI_BRIDGE_CONTRACT = yarn --silent --cwd $(EVM_DIR) hardhat deploy-bridge-token-factory --network $(1) --bridge-token-impl $(2) --near-bridge-account-id $(3)
DEPLOY_EVM_FAKE_PROVER = yarn --silent --cwd $(EVM_DIR) hardhat deploy-fake-prover --network $(1)
DEPLOY_EVM_BYTECODE = yarn --silent --cwd $(EVM_DIR) hardhat deploy-bytecode --network $(1) --bytecode $(2)
DEPLOY_EVM_ENEAR_PROXY = yarn --silent --cwd $(EVM_DIR) hardhat deploy-e-near-proxy --network $(1) --enear $(2)

ENEAR_CREATION_TEMPLATE_FILE := $(TESTING_ROOT)/bin/eNear_creation.template
ENEAR_CREATION_FILE := $(ENEAR_CREATION_TEMPLATE_FILE:.template=)

$(ENEAR_CREATION_FILE): $(ENEAR_CREATION_TEMPLATE_FILE) $(FAKE_PROVER_ADDRESS_FILE)
	cat $< | \
	sed "s/<PROVER_ADDRESS>/$(shell cat $(FAKE_PROVER_ADDRESS_FILE) | jq -r .fakeProverAddress | sed 's/^0x//')/" > $@

$(EVM_DEPLOY_RESULTS_DIR):
	mkdir -p $@

evm-compile: $(EVM_COMPILE_STAMP)
$(EVM_COMPILE_STAMP):
	@echo "Compiling EVM contracts"
	yarn --cwd $(EVM_DIR) install && \
	yarn --cwd $(EVM_DIR) hardhat compile
	touch $@

# Arguments:
# 		$(1) -  the network name
define generate_evm_deploy_rules

.PHONY: $(1)-deploy-fake-prover $(1)-deploy-enear $(1)-deploy-enear-proxy $(1)-deploy-bridge $(1)-deploy-token-impl

$(1)_DEPLOY_RESULTS_DIR := $(EVM_DEPLOY_RESULTS_DIR)/$(1)

$$($(1)_DEPLOY_RESULTS_DIR): | $(EVM_DEPLOY_RESULTS_DIR)
	mkdir -p $$@

$(1)-deploy: $(1)-deploy-bridge $(1)-deploy-enear-proxy

$(1)_BRIDGE_CONTRACT_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/omni_bridge.json
$(1)_TOKEN_IMPL_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/token_factory.json
$(1)_FAKE_PROVER_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/fake_prover.json

$(1)_ENEAR_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/eNear.json
$(1)_ENEAR_PROXY_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/eNearProxy.json


$(1)-deploy-fake-prover: $$($(1)_FAKE_PROVER_ADDRESS_FILE)
$$($(1)_FAKE_PROVER_ADDRESS_FILE): $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_FAKE_PROVER,$(1)) 2>/dev/stderr 1> $$@

$(1)-deploy-enear: $$($(1)_ENEAR_ADDRESS_FILE)
$$($(1)_ENEAR_ADDRESS_FILE): $(ENEAR_CREATION_FILE) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_BYTECODE,$(1),$(ENEAR_CREATION_FILE)) 2>/dev/stderr 1> $$@

$(1)-deploy-enear-proxy: $$($(1)_ENEAR_PROXY_ADDRESS_FILE)
$$($(1)_ENEAR_PROXY_ADDRESS_FILE): $$($(1)_ENEAR_ADDRESS_FILE) $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_ENEAR_PROXY,$(1),$$(shell cat $$($(1)_ENEAR_ADDRESS_FILE) | jq -r .contractAddress)) 2>/dev/stderr 1> $$@

$(1)-deploy-bridge: $$($(1)_BRIDGE_CONTRACT_ADDRESS_FILE)
$$($(1)_BRIDGE_CONTRACT_ADDRESS_FILE): $$($(1)_TOKEN_IMPL_ADDRESS_FILE) $(NEAR_BRIDGE_ID_FILE) $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_OMNI_BRIDGE_CONTRACT,$(1),$$(shell cat $$($(1)_TOKEN_IMPL_ADDRESS_FILE) | jq -r .tokenImplAddress),$$(shell cat $(NEAR_BRIDGE_ID_FILE) | jq -r .contract_id)) 2>/dev/stderr 1> $$@

$(1)-deploy-token-impl: $$($(1)_TOKEN_IMPL_ADDRESS_FILE)
$$($(1)_TOKEN_IMPL_ADDRESS_FILE): $(EVM_COMPILE_STAMP) | $$($(1)_DEPLOY_RESULTS_DIR)
	$$(call DEPLOY_EVM_TOKEN_IMPL,$(1)) 2>/dev/stderr 1> $$@

endef

$(foreach network,$(EVM_NETWORKS),$(eval $(call generate_evm_deploy_rules,$(network))))

##########################################################
# NEAR
##########################################################

NEAR_BINARY_DIR := $(TESTING_ROOT)/near_binary

.PHONY: near-build near-deploy

# List all expected WASM binaries
NEAR_BINARIES := evm_prover.wasm omni_bridge.wasm omni_prover.wasm omni_token.wasm token_deployer.wasm wormhole_omni_prover_proxy.wasm
NEAR_BINARY_PATHS := $(addprefix $(NEAR_BINARY_DIR)/,$(NEAR_BINARIES))
TIMESTAMP := $(shell date -u +%Y%m%d-%H%M%S)

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

$(NEAR_DEPLOY_RESULTS_DIR)/$(basename $(notdir $(1))).json: $(1) | $(NEAR_DEPLOY_RESULTS_DIR)
	./scripts/deploy-near-contract.sh $(1) $$@ $$(basename $$(notdir $(1)))-$(TIMESTAMP).testnet

$(1): $(NEAR_BUILD_STAMP)
 
endef

$(foreach binary,$(NEAR_BINARY_PATHS),$(eval $(call generate_near_deploy_rule,$(binary))))
