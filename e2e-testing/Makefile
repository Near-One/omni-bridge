
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
NEAR_CALL_RESULTS_DIR := $(TESTING_ROOT)/near_call_results

NEAR_INIT_PARAMS_FILE := $(TESTING_ROOT)/near_init_params.json

SOLANA_DIR := $(TESTING_ROOT)/../solana
SOLANA_DEPLOY_RESULTS_DIR := $(TESTING_ROOT)/solana_deploy_results

NEAR_BRIDGE_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni_bridge.json

BRIDGE_SDK_CONFIG_FILE := $(TESTING_ROOT)/bridge-sdk-config.json


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
$(1)_TOKEN_IMPL_ADDRESS_FILE := $$($(1)_DEPLOY_RESULTS_DIR)/token_impl.json
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

# List of binaries that require dynamic init args
BINARIES_WITH_DYNAMIC_ARGS := token_deployer mock_token


DEPLOY_RESULTS := $(patsubst $(NEAR_BINARY_DIR)/%.wasm,$(NEAR_DEPLOY_RESULTS_DIR)/%.json,$(NEAR_BINARY_PATHS))

NEAR_INIT_ACCOUNT_CREDENTIALS_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni-init-account.json

near-deploy: $(DEPLOY_RESULTS)

$(NEAR_DEPLOY_RESULTS_DIR):
	mkdir -p $@

$(NEAR_CALL_RESULTS_DIR):
	mkdir -p $@

near-build: $(NEAR_BINARY_PATHS)

$(NEAR_BINARY_PATHS) &:
	$(NEAR_DIR)/build.sh --output-dir $(NEAR_BINARY_DIR)

create-near-init-account: $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE)
$(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE): | $(NEAR_DEPLOY_RESULTS_DIR)
	./scripts/create-near-account.sh omni-init-$(TIMESTAMP).testnet $@

# Arguments:
# 		$(1) -  the path to the binary file
define generate_near_deploy_rules

$(1)_NAME := $$(basename $$(notdir $(1)))

# Check if the binary requires dynamic init args
ifeq ($$(filter $$($(1)_NAME),$(BINARIES_WITH_DYNAMIC_ARGS)),)

# Rule for binaries without dynamic init args
$(NEAR_DEPLOY_RESULTS_DIR)/$$($(1)_NAME).json: $(NEAR_INIT_PARAMS_FILE) $(1) $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE)  | $(NEAR_DEPLOY_RESULTS_DIR)
	./scripts/deploy-near-contract.sh $(1) $$@ $$($(1)_NAME)-$(TIMESTAMP).testnet $(NEAR_INIT_PARAMS_FILE) $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE)

else

# Rule for binaries with dynamic init args
$(NEAR_DEPLOY_RESULTS_DIR)/$$($(1)_NAME).json: $(NEAR_INIT_PARAMS_FILE) $(1) $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE) $(TESTING_ROOT)/$$($(1)_NAME)_dyn_init_args.json | $(NEAR_DEPLOY_RESULTS_DIR)
	./scripts/deploy-near-contract.sh $(1) $$@ $$($(1)_NAME)-$(TIMESTAMP).testnet $(NEAR_INIT_PARAMS_FILE) $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE) $(TESTING_ROOT)/$$($(1)_NAME)_dyn_init_args.json

endif

endef

$(foreach binary,$(NEAR_BINARY_PATHS),$(eval $(call generate_near_deploy_rules,$(binary))))

# Dynamic init args generation
$(TESTING_ROOT)/token_deployer_dyn_init_args.json: $(NEAR_DEPLOY_RESULTS_DIR)/omni_bridge.json $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE)
	CONTROLLER_ADDRESS=$$(jq -r .contract_id $(NEAR_DEPLOY_RESULTS_DIR)/omni_bridge.json) && \
	DAO_ADDRESS=$$(jq -r .account_id $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE)) && \
	echo "{\"controller\": \"$$CONTROLLER_ADDRESS\", \"dao\": \"$$DAO_ADDRESS\"}" > $@

$(TESTING_ROOT)/mock_token_dyn_init_args.json: $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE)
	OWNER_ADDRESS=$$(jq -r .account_id $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE)) && \
	echo "{\"owner_id\": \"$$OWNER_ADDRESS\"}" > $@


########################################
# Bridge New Token to Ethereum
########################################

.PHONY: near-log-metadata-call create-near-sender

BRIDGE_NEW_TOKEN_CALL_DIR := $(TESTING_ROOT)/bridge_new_token_calls

NEAR_SENDER_ACCOUNT_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni-sender.json
NEAR_OMNI_BRIDGE_CONTRACT_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni_bridge.json
NEAR_TEST_TOKEN_CONTRACT_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/mock_token.json
NEAR_RELAYER_ACCOUNT_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/omni-relayer.json
NEAR_TOKEN_DEPLOYER_CONTRACT_ID_FILE := $(NEAR_DEPLOY_RESULTS_DIR)/token_deployer.json

NEAR_LOG_METADATA_CALL_FILE := $(BRIDGE_NEW_TOKEN_CALL_DIR)/omni-log-metadata-call.json
EVM_DEPLOY_TOKEN_CALL_FILE := $(BRIDGE_NEW_TOKEN_CALL_DIR)/evm-deploy-token-call.json
NEAR_BIND_TOKEN_CALL_FILE := $(BRIDGE_NEW_TOKEN_CALL_DIR)/near-bind-token-call.json
ADD_DEPLOYER_TO_LOCKER_CALL_FILE := $(BRIDGE_NEW_TOKEN_CALL_DIR)/add-deployer-to-locker-call.json
ADD_FACTORY_TO_LOCKER_CALL_FILE := $(BRIDGE_NEW_TOKEN_CALL_DIR)/add-factory-to-locker-call.json

NEAR_TOKEN_DEPLOYMENT_PREPARE_STAMP := $(BRIDGE_NEW_TOKEN_CALL_DIR)/.near-token-deployment-prepare.stamp

SEPOLIA_CHAIN_ID := 0
SEPOLIA_CHAIN_STR := Eth

$(BRIDGE_NEW_TOKEN_CALL_DIR):
	mkdir -p $@


create-near-sender: $(NEAR_SENDER_ACCOUNT_ID_FILE)
$(NEAR_SENDER_ACCOUNT_ID_FILE): | $(NEAR_DEPLOY_RESULTS_DIR)
	./scripts/create-near-account.sh omni-sender-$(TIMESTAMP).testnet $@

create-near-relayer: $(NEAR_RELAYER_ACCOUNT_ID_FILE)
$(NEAR_RELAYER_ACCOUNT_ID_FILE): | $(NEAR_DEPLOY_RESULTS_DIR)
	./scripts/create-near-account.sh omni-relayer-$(TIMESTAMP).testnet $@

# Step 0
prepare-token-deployment: $(NEAR_TOKEN_DEPLOYMENT_PREPARE_STAMP)
$(NEAR_TOKEN_DEPLOYMENT_PREPARE_STAMP): $(ADD_DEPLOYER_TO_LOCKER_CALL_FILE) $(ADD_FACTORY_TO_LOCKER_CALL_FILE) | $(BRIDGE_NEW_TOKEN_CALL_DIR)
	touch $@

# Step 0.1
add-deployer-to-locker: $(ADD_DEPLOYER_TO_LOCKER_CALL_FILE)
$(ADD_DEPLOYER_TO_LOCKER_CALL_FILE): $(NEAR_TOKEN_DEPLOYER_CONTRACT_ID_FILE) $(NEAR_OMNI_BRIDGE_CONTRACT_ID_FILE) $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE) | $(BRIDGE_NEW_TOKEN_CALL_DIR)
	TOKEN_DEPLOYER_ID=$$(jq -r .contract_id $(NEAR_TOKEN_DEPLOYER_CONTRACT_ID_FILE)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(NEAR_OMNI_BRIDGE_CONTRACT_ID_FILE)) && \
	./scripts/call-near-contract.sh -c $$TOKEN_LOCKER_ID \
		-m add_token_deployer \
		-a "{\"chain\": \"$(SEPOLIA_CHAIN_STR)\", \"account_id\": \"$$TOKEN_DEPLOYER_ID\"}" \
		-f $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 0.2
add-ethereum-factory-to-locker: $(ADD_FACTORY_TO_LOCKER_CALL_FILE)
$(ADD_FACTORY_TO_LOCKER_CALL_FILE): $(NEAR_OMNI_BRIDGE_CONTRACT_ID_FILE) $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE) $(sepolia_BRIDGE_CONTRACT_ADDRESS_FILE) | $(BRIDGE_NEW_TOKEN_CALL_DIR)
	FACTORY_ADDRESS=$$(jq -r .bridgeAddress $(sepolia_BRIDGE_CONTRACT_ADDRESS_FILE)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(NEAR_OMNI_BRIDGE_CONTRACT_ID_FILE)) && \
	./scripts/call-near-contract.sh -c $$TOKEN_LOCKER_ID \
		-m add_factory \
		-a "{\"address\": \"$$FACTORY_ADDRESS\"}" \
		-f $(NEAR_INIT_ACCOUNT_CREDENTIALS_FILE) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


# Step 1.1
near-log-metadata-call: $(NEAR_LOG_METADATA_CALL_FILE)
$(NEAR_LOG_METADATA_CALL_FILE):  $(NEAR_SENDER_ACCOUNT_ID_FILE) $(NEAR_OMNI_BRIDGE_CONTRACT_ID_FILE) $(NEAR_TEST_TOKEN_CONTRACT_ID_FILE) $(NEAR_TOKEN_DEPLOYMENT_PREPARE_STAMP) | $(BRIDGE_NEW_TOKEN_CALL_DIR)
	TOKEN_ID=$$(jq -r .contract_id $(NEAR_TEST_TOKEN_CONTRACT_ID_FILE)) && \
	SENDER_ACCOUNT_ID=$$(jq -r .account_id $(NEAR_SENDER_ACCOUNT_ID_FILE)) && \
	SENDER_PRIVATE_KEY=$$(jq -r .private_key $(NEAR_SENDER_ACCOUNT_ID_FILE)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(NEAR_OMNI_BRIDGE_CONTRACT_ID_FILE)) && \
	bridge-cli testnet omni-connector near-log-metadata \
		--token $$TOKEN_ID \
		--near-signer $$SENDER_ACCOUNT_ID \
		--near-private-key $$SENDER_PRIVATE_KEY \
		--near-token-locker-id $$TOKEN_LOCKER_ID \
		--config-file $(BRIDGE_SDK_CONFIG_FILE) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 2.2 (ETH PRIVATE KEY AND RPC ARE SUPPLIED IN SDK CONFIG)
ethereum-deploy-token: $(EVM_DEPLOY_TOKEN_CALL_FILE)
$(EVM_DEPLOY_TOKEN_CALL_FILE): $(NEAR_LOG_METADATA_CALL_FILE) $(sepolia_BRIDGE_CONTRACT_ADDRESS_FILE) | $(BRIDGE_NEW_TOKEN_CALL_DIR)
	TX_HASH=$$(jq -r .tx_hash $(NEAR_LOG_METADATA_CALL_FILE)) && \
	ETH_BRIDGE_TOKEN_FACTORY_ADDRESS=$$(jq -r .bridgeAddress $(sepolia_BRIDGE_CONTRACT_ADDRESS_FILE)) && \
	bridge-cli testnet omni-connector evm-deploy-token \
		--eth-bridge-token-factory-address $$ETH_BRIDGE_TOKEN_FACTORY_ADDRESS \
		--source-chain-id $(SEPOLIA_CHAIN_ID) \
		--tx-hash $$TX_HASH \
		--config-file $(BRIDGE_SDK_CONFIG_FILE) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 3.2 (ETH PRIVATE KEY AND RPC ARE SUPPLIED IN SDK CONFIG)
near-bind-token: $(NEAR_BIND_TOKEN_CALL_FILE)
$(NEAR_BIND_TOKEN_CALL_FILE): $(EVM_DEPLOY_TOKEN_CALL_FILE) $(NEAR_RELAYER_ACCOUNT_ID_FILE) | $(BRIDGE_NEW_TOKEN_CALL_DIR)
	sleep 1300 && \
	TX_HASH=$$(jq -r .tx_hash $(EVM_DEPLOY_TOKEN_CALL_FILE)) && \
	RELAYER_ACCOUNT_ID=$$(jq -r .account_id $(NEAR_RELAYER_ACCOUNT_ID_FILE)) && \
	RELAYER_PRIVATE_KEY=$$(jq -r .private_key $(NEAR_RELAYER_ACCOUNT_ID_FILE)) && \
	bridge-cli testnet omni-connector evm-bind-token \
		--source-chain-id $(SEPOLIA_CHAIN_ID) \
		--tx-hash $$TX_HASH \
		--near-signer $$RELAYER_ACCOUNT_ID \
		--near-private-key $$RELAYER_PRIVATE_KEY \
		--config-file $(BRIDGE_SDK_CONFIG_FILE) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

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

