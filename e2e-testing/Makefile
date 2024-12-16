.PHONY: ethereum-deploy evm-compile deploy-eth-bridge deploy-eth-token-impl create-near-bridge-id

.PHONY: near-compile

TESTING_ROOT := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

ETHEREUM_TESTNET := sepolia
EVM_DIR := $(TESTING_ROOT)/../evm
ETHEREUM_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/ethereum_deploy_results

NEAR_DIR := $(TESTING_ROOT)/../near
NEAR_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/near_deploy_results

# File outputs
NEAR_BRIDGE_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni_bridge.json
BRIDGE_CONTRACT_ADDRESS_FILE := $(ETHEREUM_DEPLOY_RESULTS_DIR)/omni_bridge.json
TOKEN_IMPL_ADDRESS_FILE := $(ETHEREUM_DEPLOY_RESULTS_DIR)/token_factory.json
FAKE_PROVER_ADDRESS_FILE := $(ETHEREUM_DEPLOY_RESULTS_DIR)/fake_prover.json
EVM_COMPILE_STAMP := $(TESTING_ROOT)/.evm-compile.stamp

ENEAR_CREATION_TEMPLATE_FILE := $(TESTING_ROOT)/bin/eNear_creation.template
ENEAR_CREATION_FILE := $(ENEAR_CREATION_TEMPLATE_FILE:.template=)
ENEAR_ADDRESS_FILE := $(ETHEREUM_DEPLOY_RESULTS_DIR)/eNear.json
ENEAR_PROXY_ADDRESS_FILE := $(ETHEREUM_DEPLOY_RESULTS_DIR)/eNearProxy.json

# Commands
DEPLOY_EVM_TOKEN_IMPL = yarn --silent --cwd $(EVM_DIR) hardhat deploy-token-impl --network $(1)
DEPLOY_EVM_OMNI_BRIDGE_CONTRACT = yarn --silent --cwd $(EVM_DIR) hardhat deploy-bridge-token-factory --network $(1) --bridge-token-impl $(2) --near-bridge-account-id $(3)
DEPLOY_EVM_FAKE_PROVER = yarn --silent --cwd $(EVM_DIR) hardhat deploy-fake-prover --network $(1)
DEPLOY_EVM_BYTECODE = yarn --silent --cwd $(EVM_DIR) hardhat deploy-bytecode --network $(1) --bytecode $(2)
DEPLOY_EVM_ENEAR_PROXY = yarn --silent --cwd $(EVM_DIR) hardhat deploy-e-near-proxy --network $(1) --enear $(2)

$(ENEAR_CREATION_FILE): $(ENEAR_CREATION_TEMPLATE_FILE) $(FAKE_PROVER_ADDRESS_FILE)
	cat $< | \
	sed "s/<PROVER_ADDRESS>/$(shell cat $(FAKE_PROVER_ADDRESS_FILE) | jq -r .fakeProverAddress | sed 's/^0x//')/" > $@

deploy-ethereum-fake-prover: $(FAKE_PROVER_ADDRESS_FILE)
$(FAKE_PROVER_ADDRESS_FILE): $(EVM_COMPILE_STAMP)
	$(call DEPLOY_EVM_FAKE_PROVER,$(ETHEREUM_TESTNET)) 2>/dev/stderr 1> $@

deploy-ethereum-enear: $(ENEAR_ADDRESS_FILE)
$(ENEAR_ADDRESS_FILE): $(ENEAR_CREATION_FILE)
	$(call DEPLOY_EVM_BYTECODE,$(ETHEREUM_TESTNET),$(ENEAR_CREATION_FILE)) 2>/dev/stderr 1> $@

deploy-ethereum-enear-proxy: $(ENEAR_PROXY_ADDRESS_FILE)
$(ENEAR_PROXY_ADDRESS_FILE): $(ENEAR_ADDRESS_FILE) $(EVM_COMPILE_STAMP)
	$(call DEPLOY_EVM_ENEAR_PROXY,$(ETHEREUM_TESTNET),$(shell cat $(ENEAR_ADDRESS_FILE) | jq -r .contractAddress)) 2>/dev/stderr 1> $@

$(ETHEREUM_DEPLOY_RESULTS_DIR):
	mkdir -p $@

ethereum-deploy: deploy-eth-bridge

deploy-eth-bridge: $(BRIDGE_CONTRACT_ADDRESS_FILE)
$(BRIDGE_CONTRACT_ADDRESS_FILE): $(TOKEN_IMPL_ADDRESS_FILE) $(NEAR_BRIDGE_ID_FILE) $(EVM_COMPILE_STAMP) | $(ETHEREUM_DEPLOY_RESULTS_DIR)
	@echo "Deploying EVM OmniBridge contract"
	$(call DEPLOY_EVM_OMNI_BRIDGE_CONTRACT,$(ETHEREUM_TESTNET),$(shell cat $(TOKEN_IMPL_ADDRESS_FILE) | jq -r .tokenImplAddress),$(shell cat $(NEAR_BRIDGE_ID_FILE) | jq -r .contract_id)) 2>/dev/stderr 1> $@

deploy-eth-token-impl: $(TOKEN_IMPL_ADDRESS_FILE)
$(TOKEN_IMPL_ADDRESS_FILE): $(EVM_COMPILE_STAMP) | $(ETHEREUM_DEPLOY_RESULTS_DIR)
	@echo "Deploying EVM token implementation"
	$(call DEPLOY_EVM_TOKEN_IMPL,$(ETHEREUM_TESTNET)) 2>/dev/stderr 1> $@


evm-compile: $(EVM_COMPILE_STAMP)
$(EVM_COMPILE_STAMP):
	@echo "Compiling EVM contracts"
	yarn --cwd $(EVM_DIR) install && \
	yarn --cwd $(EVM_DIR) hardhat compile
	touch $@


#### NEAR

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

# Generate rules for each binary deployment
define generate_deploy_rule
$(NEAR_DEPLOY_RESULTS_DIR)/$(basename $(notdir $(1))).json: $(1) | $(NEAR_DEPLOY_RESULTS_DIR)
	./scripts/deploy-near-contract.sh $(1) $$@ $$(basename $$(notdir $(1)))-$(TIMESTAMP).testnet

$(1): $(NEAR_BUILD_STAMP)
 
endef

$(foreach binary,$(NEAR_BINARY_PATHS),$(eval $(call generate_deploy_rule,$(binary))))
