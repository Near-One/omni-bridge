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

testing_root := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

timestamp := $(shell date -u +%Y%m%d-%H%M%S)

evm_dir := $(testing_root)/../evm
evm_script_dir := $(testing_root)/evm_scripts
ethereum_deploy_results_dir := $(testing_root)/ethereum_deploy_results

near_dir := $(testing_root)/../near
near_deploy_results_dir := $(testing_root)/near_deploy_results
near_call_results_dir := $(testing_root)/near_call_results

near_init_params_file := $(testing_root)/near_init_params.json

solana_dir := $(testing_root)/../solana
solana_deploy_results_dir := $(testing_root)/solana_deploy_results

near_bridge_id_file := $(near_deploy_results_dir)/omni_bridge.json

bridge_sdk_config_file := $(testing_root)/bridge-sdk-config.json

SEPOLIA_CHAIN_ID := 0
SEPOLIA_CHAIN_STR := Eth


##########################################################
# EVM
##########################################################

.PHONY: evm-build evm-scripts-build

evm_compile_stamp := $(testing_root)/.evm-compile.stamp
evm_artifacts_dir := $(testing_root)/evm_artifacts
evm_script_compile_stamp := $(testing_root)/.evm-scripts-compile.stamp

evm_networks := sepolia arbitrumSepolia baseSepolia

evm_deploy_results_dir := $(testing_root)/evm_deploy_results

DEPLOY_EVM_TOKEN_IMPL = yarn --silent --cwd $(evm_dir) hardhat deploy-token-impl --network $(1)
DEPLOY_EVM_OMNI_BRIDGE_CONTRACT = yarn --silent --cwd $(evm_dir) hardhat deploy-bridge-token-factory --network $(1) --bridge-token-impl $(2) --near-bridge-account-id $(3)
DEPLOY_EVM_FAKE_PROVER = yarn --silent --cwd $(evm_dir) hardhat deploy-fake-prover --network $(1)
DEPLOY_EVM_ENEAR_PROXY = yarn --silent --cwd $(evm_dir) hardhat deploy-e-near-proxy --network $(1) --enear $(2)

DEPLOY_EVM_BYTECODE = yarn --silent --cwd $(evm_script_dir) hardhat deploy-bytecode --network $(1) --bytecode $(2)
DEPLOY_EVM_TEST_TOKEN = yarn --silent --cwd $(evm_script_dir) hardhat deploy-test-token --network $(1) --name $(2) --symbol $(3)

enear_creation_template_file := $(testing_root)/bin/eNear_creation.template

$(evm_deploy_results_dir):
	mkdir -p $@

evm-build: $(evm_compile_stamp)
$(evm_compile_stamp):
	@echo "Compiling EVM contracts"
	mkdir -p $(evm_artifacts_dir) && \
	yarn --cwd $(evm_dir) install --frozen-lockfile && \
	yarn --cwd $(evm_dir) hardhat compile && \
	cp -r $(evm_dir)/build/* $(evm_artifacts_dir)
	touch $@

evm-scripts-build: $(evm_script_compile_stamp)
$(evm_script_compile_stamp):
	@echo "Compiling EVM scripts"
	yarn --cwd $(evm_script_dir) install && \
	yarn --cwd $(evm_script_dir) hardhat compile
	touch $@

# Arguments:
# 		$(1) -  the network name
define generate_evm_deploy_rules

.PHONY: $(1)-deploy-fake-prover $(1)-deploy-enear $(1)-deploy-enear-proxy $(1)-deploy-bridge $(1)-deploy-token-impl $(1)-deploy-test-token

$(1)_deploy_results_dir := $(evm_deploy_results_dir)/$(1)

$$($(1)_deploy_results_dir): | $(evm_deploy_results_dir)
	mkdir -p $$@

$(1)-deploy: $(1)-deploy-bridge $(1)-deploy-enear-proxy $(1)-deploy-test-token

$(1)_bridge_contract_address_file := $$($(1)_deploy_results_dir)/omni_bridge.json
$(1)_token_impl_address_file := $$($(1)_deploy_results_dir)/token_impl.json
$(1)_fake_prover_address_file := $$($(1)_deploy_results_dir)/fake_prover.json

$(1)_enear_address_file := $$($(1)_deploy_results_dir)/eNear.json
$(1)_enear_proxy_address_file := $$($(1)_deploy_results_dir)/eNearProxy.json
$(1)_enear_creation_file := $$($(1)_deploy_results_dir)/eNear_creation

$(1)_test_token_address_file := $$($(1)_deploy_results_dir)/test_token.json

$(1)-deploy-fake-prover: $$($(1)_fake_prover_address_file)
$$($(1)_fake_prover_address_file): $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$$(call DEPLOY_EVM_FAKE_PROVER,$(1)) 2>/dev/stderr 1> $$@

$(1)-deploy-enear: $$($(1)_enear_address_file)
$$($(1)_enear_address_file): $$($(1)_enear_creation_file) $(evm_script_compile_stamp) | $$($(1)_deploy_results_dir)
	$$(call DEPLOY_EVM_BYTECODE,$(1),$$($(1)_enear_creation_file)) 2>/dev/stderr 1> $$@

$(1)-deploy-enear-proxy: $$($(1)_enear_proxy_address_file)
$$($(1)_enear_proxy_address_file): $$($(1)_enear_address_file) $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$$(call DEPLOY_EVM_ENEAR_PROXY,$(1),$$(shell cat $$($(1)_enear_address_file) | jq -r .contractAddress)) 2>/dev/stderr 1> $$@

$(1)-deploy-bridge: $$($(1)_bridge_contract_address_file)
$$($(1)_bridge_contract_address_file): $$($(1)_token_impl_address_file) $(near_bridge_id_file) $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$$(call DEPLOY_EVM_OMNI_BRIDGE_CONTRACT,$(1),$$(shell cat $$($(1)_token_impl_address_file) | jq -r .tokenImplAddress),$$(shell cat $(near_bridge_id_file) | jq -r .contract_id)) 2>/dev/stderr 1> $$@

$(1)-deploy-token-impl: $$($(1)_token_impl_address_file)
$$($(1)_token_impl_address_file): $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$$(call DEPLOY_EVM_TOKEN_IMPL,$(1)) 2>/dev/stderr 1> $$@

$$($(1)_enear_creation_file): $(enear_creation_template_file) $$($(1)_fake_prover_address_file) | $$($(1)_deploy_results_dir)
	cat $$< | \
	sed "s/<PROVER_ADDRESS>/$$(shell cat $$($(1)_fake_prover_address_file) | jq -r .fakeProverAddress | sed 's/^0x//')/" > $$@


$(1)-deploy-test-token: $$($(1)_test_token_address_file)
$$($(1)_test_token_address_file): $(evm_script_compile_stamp) | $$($(1)_deploy_results_dir)
	$$(call DEPLOY_EVM_TEST_TOKEN,$(1),E2ETestToken-$(timestamp),E2ETT-$(timestamp)) 2>/dev/stderr 1> $$@

endef

$(foreach network,$(evm_networks),$(eval $(call generate_evm_deploy_rules,$(network))))

##########################################################
# NEAR
##########################################################

near_binary_dir := $(testing_root)/near_artifacts

.PHONY: near-build near-deploy

# List all expected WASM binaries
near_binaries := evm_prover.wasm omni_bridge.wasm omni_prover.wasm token_deployer.wasm wormhole_omni_prover_proxy.wasm mock_token.wasm
near_binary_paths := $(addprefix $(near_binary_dir)/,$(near_binaries))

# List of binaries that require dynamic init args
binaries_with_dynamic_args := token_deployer mock_token omni_bridge

deploy_results := $(patsubst $(near_binary_dir)/%.wasm,$(near_deploy_results_dir)/%.json,$(near_binary_paths))

near_init_account_credentials_file := $(near_deploy_results_dir)/omni-init-account.json
near_dao_account_credentials_file := $(near_deploy_results_dir)/omni-dao-account.json

near-deploy: $(deploy_results)

$(near_deploy_results_dir):
	mkdir -p $@

$(near_call_results_dir):
	mkdir -p $@

near-build: $(near_binary_paths)

$(near_binary_paths) &:
	$(near_dir)/build.sh --output-dir $(near_binary_dir)

create-near-init-account: $(near_init_account_credentials_file)
$(near_init_account_credentials_file): | $(near_deploy_results_dir)
	./scripts/create-near-account.sh omni-init-$(timestamp).testnet $@

create-dao-account: $(near_dao_account_credentials_file)
$(near_dao_account_credentials_file): | $(near_deploy_results_dir)
	./scripts/create-near-account.sh omni-dao-$(timestamp).testnet $@

# Arguments:
# 		$(1) -  the path to the binary file
define generate_near_deploy_rules

$(1)_name := $$(basename $$(notdir $(1)))

# Check if the binary requires dynamic init args
ifeq ($$(filter $$($(1)_name),$(binaries_with_dynamic_args)),)

# Rule for binaries without dynamic init args
$(near_deploy_results_dir)/$$($(1)_name).json: $(near_init_params_file) $(1) $(near_init_account_credentials_file)  | $(near_deploy_results_dir)
	./scripts/deploy-near-contract.sh $(1) $$@ $$($(1)_name)-$(timestamp).testnet $(near_init_params_file) $(near_init_account_credentials_file)

else

# Rule for binaries with dynamic init args
$(near_deploy_results_dir)/$$($(1)_name).json: $(near_init_params_file) $(1) $(near_init_account_credentials_file) $(testing_root)/$$($(1)_name)_dyn_init_args.json | $(near_deploy_results_dir)
	./scripts/deploy-near-contract.sh $(1) $$@ $$($(1)_name)-$(timestamp).testnet $(near_init_params_file) $(near_init_account_credentials_file) $(testing_root)/$$($(1)_name)_dyn_init_args.json

endif

endef

$(foreach binary,$(near_binary_paths),$(eval $(call generate_near_deploy_rules,$(binary))))

# Dynamic init args generation
$(testing_root)/token_deployer_dyn_init_args.json: $(near_deploy_results_dir)/omni_bridge.json $(near_init_account_credentials_file)
	CONTROLLER_ADDRESS=$$(jq -r .contract_id $(near_deploy_results_dir)/omni_bridge.json) && \
	DAO_ADDRESS=$$(jq -r .account_id $(near_init_account_credentials_file)) && \
	echo "{\"controller\": \"$$CONTROLLER_ADDRESS\", \"dao\": \"$$DAO_ADDRESS\"}" > $@

$(testing_root)/mock_token_dyn_init_args.json: $(near_init_account_credentials_file)
	OWNER_ADDRESS=$$(jq -r .account_id $(near_init_account_credentials_file)) && \
	echo "{\"owner_id\": \"$$OWNER_ADDRESS\"}" > $@

$(testing_root)/omni_bridge_dyn_init_args.json: $(near_deploy_results_dir)/omni_prover.json
	PROVER_ADDRESS=$$(jq -r .contract_id $(near_deploy_results_dir)/omni_prover.json) && \
	echo "{\"prover_account\": \"$$PROVER_ADDRESS\"}" > $@

prover_dau_grant_call_file := $(near_deploy_results_dir)/omni-prover-dau-grant-call.json
omni-prover-dau-grant: $(prover_dau_grant_call_file)
$(prover_dau_grant_call_file): $(near_init_account_credentials_file) $(near_dao_account_credentials_file) $(near_deploy_results_dir)/omni_prover.json
	OMNI_PROVER_ACCOUNT_ID=$$(jq -r .contract_id $(near_deploy_results_dir)/omni_prover.json) && \
	DAO_ACCOUNT_ID=$$(jq -r .account_id $(near_dao_account_credentials_file)) && \
	./scripts/call-near-contract.sh -c $$OMNI_PROVER_ACCOUNT_ID \
		-m acl_grant_role \
		-a "{\"role\": \"DAO\", \"account_id\": \"$$DAO_ACCOUNT_ID\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


evm_prover_setup_call_file := $(near_deploy_results_dir)/evm-prover-setup-call.json
evm-prover-setup: $(evm_prover_setup_call_file)
$(evm_prover_setup_call_file): $(near_deploy_results_dir)/omni_prover.json $(near_deploy_results_dir)/evm_prover.json $(prover_dau_grant_call_file)
	OMNI_PROVER_ACCOUNT_ID=$$(jq -r .contract_id $(near_deploy_results_dir)/omni_prover.json) && \
	EVM_PROVER_ACCOUNT_ID=$$(jq -r .contract_id $(near_deploy_results_dir)/evm_prover.json) && \
	./scripts/call-near-contract.sh -c $$OMNI_PROVER_ACCOUNT_ID \
		-m add_prover \
		-a "{\"account_id\": \"$$EVM_PROVER_ACCOUNT_ID\", \"prover_id\": \"$(SEPOLIA_CHAIN_STR)\"}" \
		-f $(near_dao_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


########################################
# Bridge New Token to Ethereum
########################################

.PHONY: near-log-metadata-call create-near-sender

bridge_new_token_call_dir := $(testing_root)/bridge_new_token_calls

near_sender_account_id_file := $(near_deploy_results_dir)/omni-sender.json
near_omni_bridge_contract_id_file := $(near_deploy_results_dir)/omni_bridge.json
near_test_token_contract_id_file := $(near_deploy_results_dir)/mock_token.json
near_relayer_account_id_file := $(near_deploy_results_dir)/omni-relayer.json
near_token_deployer_contract_id_file := $(near_deploy_results_dir)/token_deployer.json

near_log_metadata_call_file := $(bridge_new_token_call_dir)/omni-log-metadata-call.json
evm_deploy_token_call_file := $(bridge_new_token_call_dir)/evm-deploy-token-call.json
near_bind_token_call_file := $(bridge_new_token_call_dir)/near-bind-token-call.json
add_deployer_to_locker_call_file := $(bridge_new_token_call_dir)/add-deployer-to-locker-call.json
add_factory_to_locker_call_file := $(bridge_new_token_call_dir)/add-factory-to-locker-call.json

near_token_deployment_prepare_stamp := $(bridge_new_token_call_dir)/.near-token-deployment-prepare.stamp

$(bridge_new_token_call_dir):
	mkdir -p $@


create-near-sender: $(near_sender_account_id_file)
$(near_sender_account_id_file): | $(near_deploy_results_dir)
	./scripts/create-near-account.sh omni-sender-$(timestamp).testnet $@

create-near-relayer: $(near_relayer_account_id_file)
$(near_relayer_account_id_file): | $(near_deploy_results_dir)
	./scripts/create-near-account.sh omni-relayer-$(timestamp).testnet $@

bridge-new-token-to-ethereum: near-bind-token

# Step 0
prepare-token-deployment: $(near_token_deployment_prepare_stamp)
$(near_token_deployment_prepare_stamp): $(add_deployer_to_locker_call_file) $(add_factory_to_locker_call_file) $(evm_prover_setup_call_file) | $(bridge_new_token_call_dir)
	touch $@

# Step 0.1
add-deployer-to-locker: $(add_deployer_to_locker_call_file)
$(add_deployer_to_locker_call_file): $(near_token_deployer_contract_id_file) $(near_omni_bridge_contract_id_file) $(near_init_account_credentials_file) | $(bridge_new_token_call_dir)
	TOKEN_DEPLOYER_ID=$$(jq -r .contract_id $(near_token_deployer_contract_id_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(near_omni_bridge_contract_id_file)) && \
	./scripts/call-near-contract.sh -c $$TOKEN_LOCKER_ID \
		-m add_token_deployer \
		-a "{\"chain\": \"$(SEPOLIA_CHAIN_STR)\", \"account_id\": \"$$TOKEN_DEPLOYER_ID\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 0.2
add-ethereum-factory-to-locker: $(add_factory_to_locker_call_file)
$(add_factory_to_locker_call_file): $(near_omni_bridge_contract_id_file) $(near_init_account_credentials_file) $(sepolia_bridge_contract_address_file) | $(bridge_new_token_call_dir)
	FACTORY_ADDRESS=$$(jq -r .bridgeAddress $(sepolia_bridge_contract_address_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(near_omni_bridge_contract_id_file)) && \
	./scripts/call-near-contract.sh -c $$TOKEN_LOCKER_ID \
		-m add_factory \
		-a "{\"address\": \"$$FACTORY_ADDRESS\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


# Step 1.1
near-log-metadata-call: $(near_log_metadata_call_file)
$(near_log_metadata_call_file):  $(near_sender_account_id_file) $(near_omni_bridge_contract_id_file) $(near_test_token_contract_id_file) $(near_token_deployment_prepare_stamp) | $(bridge_new_token_call_dir)
	TOKEN_ID=$$(jq -r .contract_id $(near_test_token_contract_id_file)) && \
	SENDER_ACCOUNT_ID=$$(jq -r .account_id $(near_sender_account_id_file)) && \
	SENDER_PRIVATE_KEY=$$(jq -r .private_key $(near_sender_account_id_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(near_omni_bridge_contract_id_file)) && \
	bridge-cli testnet omni-connector near-log-metadata \
		--token $$TOKEN_ID \
		--near-signer $$SENDER_ACCOUNT_ID \
		--near-private-key $$SENDER_PRIVATE_KEY \
		--near-token-locker-id $$TOKEN_LOCKER_ID \
		--config-file $(bridge_sdk_config_file) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 2.2 (ETH PRIVATE KEY AND RPC ARE SUPPLIED IN SDK CONFIG)
ethereum-deploy-token: $(evm_deploy_token_call_file)
$(evm_deploy_token_call_file): $(near_log_metadata_call_file) $(sepolia_bridge_contract_address_file) | $(bridge_new_token_call_dir)
	TX_HASH=$$(jq -r .tx_hash $(near_log_metadata_call_file)) && \
	ETH_BRIDGE_TOKEN_FACTORY_ADDRESS=$$(jq -r .bridgeAddress $(sepolia_bridge_contract_address_file)) && \
	bridge-cli testnet omni-connector evm-deploy-token \
		--eth-bridge-token-factory-address $$ETH_BRIDGE_TOKEN_FACTORY_ADDRESS \
		--source-chain-id $(SEPOLIA_CHAIN_ID) \
		--tx-hash $$TX_HASH \
		--config-file $(bridge_sdk_config_file) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 3.2 (ETH PRIVATE KEY AND RPC ARE SUPPLIED IN SDK CONFIG)
near-bind-token: $(near_bind_token_call_file)
$(near_bind_token_call_file): $(evm_deploy_token_call_file) $(near_relayer_account_id_file) | $(bridge_new_token_call_dir)
	sleep 1300 && \
	TX_HASH=$$(jq -r .tx_hash $(evm_deploy_token_call_file)) && \
	RELAYER_ACCOUNT_ID=$$(jq -r .account_id $(near_relayer_account_id_file)) && \
	RELAYER_PRIVATE_KEY=$$(jq -r .private_key $(near_relayer_account_id_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(near_omni_bridge_contract_id_file)) && \
	bridge-cli testnet omni-connector evm-bind-token \
		--source-chain-id $(SEPOLIA_CHAIN_ID) \
		--tx-hash $$TX_HASH \
		--near-signer $$RELAYER_ACCOUNT_ID \
		--near-private-key $$RELAYER_PRIVATE_KEY \
		--near-token-locker-id $$TOKEN_LOCKER_ID \
		--config-file $(bridge_sdk_config_file) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

########################################
# Solana
########################################

solana_build_stamp := $(testing_root)/.solana-build.stamp
solana_artifacts_dir := $(testing_root)/solana_artifacts

solana_programs := bridge_token_factory
solana_programs_keypairs := $(foreach program,$(solana_programs),$(solana_dir)/$(program)/target/deploy/$(program)-keypair.json)
solana_programs_binaries := $(foreach program,$(solana_programs),$(solana_artifacts_dir)/$(program)/target/deploy/$(program).so)

solana-build: $(solana_build_stamp)
$(solana_build_stamp): $(solana_programs_keypairs) $(solana_programs_binaries)
	touch $@

# Arguments:
# 		$(1) -  the program name
define generate_solana_build_rules

$(solana_dir)/$(1)/target/deploy/$(1)-keypair.json: $(testing_root)/$(1)-keypair.json
	mkdir -p $$(dir $$@) && \
	cp $$< $$@

$(solana_artifacts_dir)/$(1)/target/deploy/$(1).so: $(solana_dir)/$(1)/target/deploy/$(1)-keypair.json
	mkdir -p $(solana_artifacts_dir)/$(1) && \
	cd $(solana_dir)/$(1) && \
	anchor build && \
	cp -r $(solana_dir)/$(1)/target/* $(solana_artifacts_dir)/$(1)

endef 

$(foreach program,$(solana_programs),$(eval $(call generate_solana_build_rules,$(program))))

