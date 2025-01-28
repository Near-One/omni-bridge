# Pipeline: Bridge NEAR Token to Ethereum
pipeline1_call_dir := $(common_generated_dir)/bridge-token-near-to-evm

# Clean target
.PHONY: clean-bridge-token-near-to-evm
clean-bridge-token-near-to-evm:
	$(call description,Cleaning bridge pipeline artifacts)
	rm -rf $(pipeline1_call_dir)

# Account and contract ID files
pipeline1_sender_account_file := $(common_near_deploy_results_dir)/omni-sender.json
pipeline1_bridge_contract_file := $(common_near_deploy_results_dir)/omni_bridge.json
pipeline1_test_token_file := $(common_near_deploy_results_dir)/mock_token.json
pipeline1_relayer_account_file := $(common_near_deploy_results_dir)/omni-relayer.json
pipeline1_token_deployer_file := $(common_near_deploy_results_dir)/token_deployer.json

# Call files
pipeline1_add_deployer_file := $(pipeline1_call_dir)/00-1_add-deployer-to-locker-call.json
pipeline1_add_factory_file := $(pipeline1_call_dir)/00-2_add-factory-to-locker-call.json
pipeline1_log_metadata_file := $(pipeline1_call_dir)/01_omni-log-metadata-call.json
pipeline1_evm_deploy_token_file := $(pipeline1_call_dir)/02_evm-deploy-token-call.json
pipeline1_near_bind_token_file := $(pipeline1_call_dir)/03_near-bind-token-call.json

pipeline1_prepare_stamp := $(pipeline1_call_dir)/.prepare.stamp

pipeline1_verify_bridge_token_report := $(pipeline1_call_dir)/verify-bridge-token-report.txt

$(pipeline1_call_dir):
	mkdir -p $@

# Account creation rules
.PHONY: create-near-sender
create-near-sender: $(pipeline1_sender_account_file)
$(pipeline1_sender_account_file): | $(common_near_deploy_results_dir)
	$(call description,Creating NEAR sender account)
	$(common_scripts_dir)/create-near-account.sh omni-sender-$(common_timestamp).testnet $@

.PHONY: create-near-relayer
create-near-relayer: $(pipeline1_relayer_account_file)
$(pipeline1_relayer_account_file): | $(common_near_deploy_results_dir)
	$(call description,Creating NEAR relayer account)
	$(common_scripts_dir)/create-near-account.sh omni-relayer-$(common_timestamp).testnet $@

# Main pipeline target
.PHONY: bridge-token-near-to-evm
bridge-token-near-to-evm: verify-bridge-token-near-to-evm

# Step 0: Prepare token deployment
.PHONY: prepare-token-deployment
prepare-token-deployment: $(pipeline1_prepare_stamp)
$(pipeline1_prepare_stamp): $(pipeline1_add_deployer_file) $(pipeline1_add_factory_file) $(near_evm_prover_setup_call_file) | $(pipeline1_call_dir)
	$(call description,Token deployment preparation complete)
	touch $@

# Step 0.1: Add deployer to locker
.PHONY: add-deployer-to-locker
add-deployer-to-locker: $(pipeline1_add_deployer_file)
$(pipeline1_add_deployer_file): $(pipeline1_token_deployer_file) $(pipeline1_bridge_contract_file) $(near_init_account_credentials_file) | $(pipeline1_call_dir)
	$(call description,Bridge NEAR Token to Ethereum. Step 0.1: Adding token deployer to locker)
	TOKEN_DEPLOYER_ID=$$(jq -r .contract_id $(pipeline1_token_deployer_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(pipeline1_bridge_contract_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$TOKEN_LOCKER_ID \
		-m add_token_deployer \
		-a "{\"chain\": \"$(COMMON_SEPOLIA_CHAIN_STR)\", \"account_id\": \"$$TOKEN_DEPLOYER_ID\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 0.2: Add Ethereum factory to locker
.PHONY: add-ethereum-factory-to-locker
add-ethereum-factory-to-locker: $(pipeline1_add_factory_file)
$(pipeline1_add_factory_file): $(pipeline1_bridge_contract_file) $(near_init_account_credentials_file) $(sepolia_bridge_contract_address_file) | $(pipeline1_call_dir)
	$(call description,Bridge NEAR Token to Ethereum. Step 0.2: Adding Ethereum factory to locker)
	FACTORY_ADDRESS=$$(jq -r .bridgeAddress $(sepolia_bridge_contract_address_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(pipeline1_bridge_contract_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$TOKEN_LOCKER_ID \
		-m add_factory \
		-a "{\"address\": \"$$FACTORY_ADDRESS\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 1: Log metadata
.PHONY: near-log-metadata-call
near-log-metadata-call: $(pipeline1_log_metadata_file)
$(pipeline1_log_metadata_file): $(pipeline1_sender_account_file) $(pipeline1_bridge_contract_file) $(pipeline1_test_token_file) $(pipeline1_prepare_stamp) | $(pipeline1_call_dir)
	$(call description,Bridge NEAR Token to Ethereum. Step 1: Logging token metadata)
	TOKEN_ID=$$(jq -r .contract_id $(pipeline1_test_token_file)) && \
	SENDER_ACCOUNT_ID=$$(jq -r .account_id $(pipeline1_sender_account_file)) && \
	SENDER_PRIVATE_KEY=$$(jq -r .private_key $(pipeline1_sender_account_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(pipeline1_bridge_contract_file)) && \
	bridge-cli testnet omni-connector log-metadata \
		--token near:$$TOKEN_ID \
		--near-signer $$SENDER_ACCOUNT_ID \
		--near-private-key $$SENDER_PRIVATE_KEY \
		--near-token-locker-id $$TOKEN_LOCKER_ID \
		--config-file $(common_bridge_sdk_config_file) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

# Step 2: Deploy token on Ethereum
.PHONY: ethereum-deploy-token
ethereum-deploy-token: $(pipeline1_evm_deploy_token_file)
$(pipeline1_evm_deploy_token_file): $(pipeline1_log_metadata_file) $(sepolia_bridge_contract_address_file) | $(pipeline1_call_dir)
	$(call description,Bridge NEAR Token to Ethereum. Step 2: Deploying token on Ethereum)
	TX_HASH=$$(jq -r .tx_hash $(pipeline1_log_metadata_file)) && \
	ETH_BRIDGE_TOKEN_FACTORY_ADDRESS=$$(jq -r .bridgeAddress $(sepolia_bridge_contract_address_file)) && \
	bridge-cli testnet omni-connector deploy-token \
		--chain $(COMMON_SEPOLIA_CHAIN_STR) \
		--source-chain $(COMMON_NEAR_CHAIN_STR) \
		--tx-hash $$TX_HASH \
		--eth-bridge-token-factory-address $$ETH_BRIDGE_TOKEN_FACTORY_ADDRESS \
		--config-file $(common_bridge_sdk_config_file) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


# Step 3: Bind token on NEAR
.PHONY: near-bind-token
near-bind-token: $(pipeline1_near_bind_token_file)
$(pipeline1_near_bind_token_file): $(pipeline1_evm_deploy_token_file) $(pipeline1_relayer_account_file) | $(pipeline1_call_dir)
	$(call description,Waiting for Ethereum transaction being captured by relayer)
	$(call progress_wait,1300)
	$(call description,Bridge NEAR Token to Ethereum. Step 3: Binding token on NEAR)
	TX_HASH=$$(jq -r .tx_hash $(pipeline1_evm_deploy_token_file)) && \
	RELAYER_ACCOUNT_ID=$$(jq -r .account_id $(pipeline1_relayer_account_file)) && \
	RELAYER_PRIVATE_KEY=$$(jq -r .private_key $(pipeline1_relayer_account_file)) && \
	TOKEN_LOCKER_ID=$$(jq -r .contract_id $(pipeline1_bridge_contract_file)) && \
	bridge-cli testnet omni-connector bind-token \
		--chain $(COMMON_SEPOLIA_CHAIN_STR) \
		--tx-hash $$TX_HASH \
		--near-signer $$RELAYER_ACCOUNT_ID \
		--near-private-key $$RELAYER_PRIVATE_KEY \
		--near-token-locker-id $$TOKEN_LOCKER_ID \
		--config-file $(common_bridge_sdk_config_file) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@ 

# Step 4: Verify the correctness of the token bridging
.PHONY: verify-bridge-token-near-to-evm
verify-bridge-token-near-to-evm: $(pipeline1_verify_bridge_token_report)
$(pipeline1_verify_bridge_token_report): $(pipeline1_near_bind_token_file) $(common_tools_compile_stamp)
	$(call description,Bridge NEAR Token to Ethereum. Verification)
	NEAR_TOKEN_ID=$$(jq -r .contract_id $(pipeline1_test_token_file)) && \
	NEAR_LOCKER_ID=$$(jq -r .contract_id $(pipeline1_bridge_contract_file)) && \
	TOKEN_TX_HASH=$$(jq -r .tx_hash $(pipeline1_evm_deploy_token_file)) && \
	yarn --cwd $(common_tools_dir) --silent verify-bridge-token-near-to-evm \
		--tx-dir $(pipeline1_call_dir) \
		--near-token $$NEAR_TOKEN_ID \
		--chain-kind $(COMMON_SEPOLIA_CHAIN_STR) \
		--near-locker $$NEAR_LOCKER_ID \
		--token-tx $$TOKEN_TX_HASH | tee $@
