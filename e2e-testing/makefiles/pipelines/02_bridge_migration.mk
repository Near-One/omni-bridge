pipeline2_call_dir := $(common_generated_dir)/02_bridge-migration

$(pipeline2_call_dir):
	mkdir -p $@

# Contract deployment and account files
pipeline2_old_bridge_token_factory_file := $(common_near_deploy_results_dir)/bridge_token_factory.json
pipeline2_omni_bridge_contract_file := $(common_near_deploy_results_dir)/omni_bridge.json
pipeline2_eth_omni_bridge_contract_file := $(sepolia_deploy_results_dir)/omni_bridge.json
pipeline2_user_account_file := $(common_near_deploy_results_dir)/near-user.json
pipeline2_eth_user_account_file := $(sepolia_deploy_results_dir)/eth-user.json
pipeline2_near_relayer_account_file := $(common_near_deploy_results_dir)/near-relayer.json

# Call files
pipeline2_bridge_factory_controller_grant_omni_init_call_file := $(pipeline2_call_dir)/01_bridge-factory-dao-grant-call.json
pipeline2_mint_token_to_omni_bridge_eth_call_file := $(pipeline2_call_dir)/12_mint-token-to-omni-bridge-call.json
pipeline2_omni_bridge_storage_deposit_call_file := $(pipeline2_call_dir)/13_omni-bridge-storage-deposit-call.json
pipeline2_bridge_factory_dao_grant_call_file := $(pipeline2_call_dir)/08_bridge-factory-controller-grant-omni-init-call.json
pipeline2_old_bridge_token_deploy_file := $(pipeline2_call_dir)/03_old-bridge-token-deploy-call.json
pipeline2_bridge_token_controller_init_call_file := $(pipeline2_call_dir)/05_bridge-token-controller-init-call.json
pipeline2_mint_token_to_user_near_call_file := $(pipeline2_call_dir)/06_mint-token-to-user-near-call.json
pipeline2_omni_bridge_dao_grant_call_file := $(pipeline2_call_dir)/02_omni-bridge-dao-grant-call.json
pipeline2_add_token_to_omni_bridge_call_file := $(pipeline2_call_dir)/11_add-token-to-omni-bridge-call.json
pipeline2_bridge_factory_controller_grant_omni_bridge_call_file := $(pipeline2_call_dir)/09_bridge-factory-controller-grant-omni-bridge-call.json
pipeline2_bridge_token_controller_omni_bridge_call_file := $(pipeline2_call_dir)/10_bridge-token-controller-omni-bridge-call.json
pipeline2_withdraw_token_on_near_call_file := $(pipeline2_call_dir)/14_withdraw-token-on-near-call.json
pipeline2_sign_withdraw_token_on_near_call_file := $(pipeline2_call_dir)/15_sign-withdraw-token-on-near-call.json
pipeline2_eth_fin_transfer_call_file := $(pipeline2_call_dir)/16_eth-fin-transfer-call.json
pipeline2_old_bridge_token_set_metadata_call_file := $(pipeline2_call_dir)/04_old-bridge-token-set-metadata-call.json
pipeline2_verify_correctness_report := $(pipeline2_call_dir)/correctness-report.txt

# Variables
pipeline2_old_bridge_token_deploy_deposit := "6 NEAR"
pipeline2_omni_bridge_add_token_deposit := "0.00125 NEAR"
pipeline2_truncated_timestamp := $(shell date -u +%Y%m%d%H%M)
pipeline2_test_token_amount := 100
pipeline2_storage_deposit := "0.00125 NEAR"


# Binary files
pipeline2_old_bridge_token_binary_file := $(near_binary_dir)/bridge_token.wasm # This file is to be fetched in GA pipeline
pipeline2_old_bridge_token_factory_binary_file := $(near_binary_dir)/bridge_token_factory.wasm # This file is to be fetched in GA pipeline

.PHONY: pipeline2
02-bridge-migration: pipeline2-verify-correctness


.PHONY: deploy-old-bridge-token-factory
deploy-old-bridge-token-factory: $(pipeline2_old_bridge_token_factory_file)
$(pipeline2_old_bridge_token_factory_file): $(near_init_params_file) $(near_init_account_credentials_file) $(pipeline2_old_bridge_token_factory_binary_file) $(pipeline2_old_bridge_token_binary_file) | $(pipeline2_call_dir)
	$(call description,Deploying old bridge token factory contract)
	export BRIDGE_TOKEN_BINARY=$(pipeline2_old_bridge_token_binary_file) && \
	$(common_scripts_dir)/deploy-near-contract.sh $(near_init_params_file) $(near_init_account_credentials_file) $(pipeline2_old_bridge_token_factory_binary_file) btf$(pipeline2_truncated_timestamp).testnet $@


.PHONY: bridge-factory-dao-grant
bridge-factory-dao-grant: $(pipeline2_bridge_factory_dao_grant_call_file)
$(pipeline2_bridge_factory_dao_grant_call_file): $(near_init_account_credentials_file) $(near_dao_account_credentials_file) $(pipeline2_old_bridge_token_factory_file)
	$(call description,Granting DAO role to bridge factory)
	BRIDGE_FACTORY_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_factory_file)) && \
	DAO_ACCOUNT_ID=$$(jq -r .account_id $(near_dao_account_credentials_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$BRIDGE_FACTORY_ADDRESS \
		-m acl_grant_role \
		-a "{\"role\": \"DAO\", \"account_id\": \"$$DAO_ACCOUNT_ID\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: create-near-relayer
create-near-relayer: $(pipeline2_near_relayer_account_file)
$(pipeline2_near_relayer_account_file): | $(common_near_deploy_results_dir)
	$(call description,Creating NEAR relayer account)
	$(common_scripts_dir)/create-near-account.sh near-relayer-$(common_timestamp).testnet $@


.PHONY: set-bridge-token-metadata
set-bridge-token-metadata: $(pipeline2_old_bridge_token_set_metadata_call_file)
$(pipeline2_old_bridge_token_set_metadata_call_file): $(pipeline2_old_bridge_token_deploy_file) $(pipeline2_bridge_factory_dao_grant_call_file) $(pipeline2_old_bridge_token_factory_file) $(sepolia_test_token_address_file) $(near_dao_account_credentials_file) | $(pipeline2_call_dir)
	$(call description,Setting metadata for old bridge token)
	TOKEN_ADDRESS=$$(jq -r '.contractAddress | sub("^.{2}"; "")' $(sepolia_test_token_address_file)) && \
	TOKEN_FACTORY_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_factory_file)) && \
	TOKEN_NAME=$$(jq -r .name $(sepolia_test_token_address_file)) && \
	TOKEN_SYMBOL=$$(jq -r .symbol $(sepolia_test_token_address_file)) && \
	TOKEN_DECIMALS=18 && \
	$(common_scripts_dir)/call-near-contract.sh -c $$TOKEN_FACTORY_ADDRESS \
		-m set_metadata \
		-a "{\"address\": \"$$TOKEN_ADDRESS\", \"name\": \"$$TOKEN_NAME\", \"symbol\": \"$$TOKEN_SYMBOL\", \"decimals\": $$TOKEN_DECIMALS}" \
		-f $(near_dao_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: bridge-factory-controller-grant-omni-init
bridge-factory-controller-grant-omni-init: $(pipeline2_bridge_factory_controller_grant_omni_init_call_file)
$(pipeline2_bridge_factory_controller_grant_omni_init_call_file): $(near_init_account_credentials_file) $(pipeline2_old_bridge_token_set_metadata_call_file) $(pipeline2_old_bridge_token_factory_file)
	$(call description,Granting controller role of bridge factory to omni init)
	BRIDGE_FACTORY_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_factory_file)) && \
	FACTORY_CONTROLLER_ADDRESS=$$(jq -r .account_id $(near_init_account_credentials_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$BRIDGE_FACTORY_ADDRESS \
		-m acl_grant_role \
		-a "{\"role\": \"Controller\", \"account_id\": \"$$FACTORY_CONTROLLER_ADDRESS\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: omni-bridge-dao-grant
omni-bridge-dao-grant: $(pipeline2_omni_bridge_dao_grant_call_file)
$(pipeline2_omni_bridge_dao_grant_call_file): $(near_init_account_credentials_file) $(near_dao_account_credentials_file) $(pipeline2_omni_bridge_contract_file)
	$(call description,Granting DAO role of Omni Bridge to DAO account)
	OMNI_BRIDGE_ADDRESS=$$(jq -r .contract_id $(pipeline2_omni_bridge_contract_file)) && \
	DAO_ACCOUNT_ID=$$(jq -r .account_id $(near_dao_account_credentials_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$OMNI_BRIDGE_ADDRESS \
		-m acl_grant_role \
		-a "{\"role\": \"DAO\", \"account_id\": \"$$DAO_ACCOUNT_ID\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: create-near-user
create-near-user: $(pipeline2_user_account_file)
$(pipeline2_user_account_file): | $(common_near_deploy_results_dir)
	$(call description,Creating NEAR user account)
	$(common_scripts_dir)/create-near-account.sh near-user-$(common_timestamp).testnet $@


.PHONY: deploy-old-bridge-token
deploy-old-bridge-token: $(pipeline2_old_bridge_token_deploy_file)
$(pipeline2_old_bridge_token_deploy_file): $(pipeline2_old_bridge_token_factory_file) $(sepolia_test_token_address_file) $(near_init_account_credentials_file) | $(pipeline2_call_dir)
	$(call description,Deploying old bridge token contract)
	TOKEN_FACTORY_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_factory_file)) && \
	SEPOLIA_TOKEN_ADDRESS=$$(jq -r '.contractAddress | sub("^.{2}"; "")' $(sepolia_test_token_address_file)) && \
	TOKEN_CONTRACT_ID=$$(echo $$SEPOLIA_TOKEN_ADDRESS.$$TOKEN_FACTORY_ADDRESS | tr '[:upper:]' '[:lower:]') && \
	$(common_scripts_dir)/call-near-contract.sh -c $$TOKEN_FACTORY_ADDRESS \
		-m deploy_bridge_token \
		-a "{\"address\": \"$$SEPOLIA_TOKEN_ADDRESS\"}" \
		-f $(near_init_account_credentials_file) \
		-d $(pipeline2_old_bridge_token_deploy_deposit) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\", \"contract_id\": \"$$TOKEN_CONTRACT_ID\"}" > $@


.PHONY: set-bridge-token-controller-init
set-bridge-token-controller-init: $(pipeline2_bridge_token_controller_init_call_file)
$(pipeline2_bridge_token_controller_init_call_file): $(pipeline2_bridge_factory_controller_grant_omni_init_call_file) $(pipeline2_old_bridge_token_deploy_file) $(pipeline2_old_bridge_token_factory_file) $(near_init_account_credentials_file) | $(pipeline2_call_dir)
	$(call description,Granting controller role of bridge token to omni-init)
	TOKEN_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_deploy_file)) && \
	FACTORY_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_factory_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$FACTORY_ADDRESS \
		-m set_controller_for_tokens \
		-a "{\"tokens_account_id\": [\"$$TOKEN_ADDRESS\"]}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: mint-token-to-user-near
mint-token-to-user-near: $(pipeline2_mint_token_to_user_near_call_file)
$(pipeline2_mint_token_to_user_near_call_file): $(pipeline2_bridge_token_controller_init_call_file) $(pipeline2_old_bridge_token_set_metadata_call_file) $(pipeline2_old_bridge_token_deploy_file) $(pipeline2_user_account_file) $(near_init_account_credentials_file) | $(pipeline2_call_dir)
	$(call description,Minting token to NEAR user)
	TOKEN_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_deploy_file)) && \
	USER_ADDRESS=$$(jq -r .account_id $(pipeline2_user_account_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$TOKEN_ADDRESS \
		-m mint \
		-a "{\"amount\": \"$(pipeline2_test_token_amount)\", \"account_id\": \"$$USER_ADDRESS\"}" \
		-f $(near_init_account_credentials_file) \
		-d $(pipeline2_storage_deposit) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: bridge-factory-controller-grant-omni-bridge
bridge-factory-controller-grant-omni-bridge: $(pipeline2_bridge_factory_controller_grant_omni_bridge_call_file)
$(pipeline2_bridge_factory_controller_grant_omni_bridge_call_file): $(pipeline2_mint_token_to_user_near_call_file) $(near_init_account_credentials_file) $(pipeline2_omni_bridge_contract_file) $(pipeline2_old_bridge_token_factory_file)
	$(call description,Granting controller role of bridge factory to Omni Bridge)
	BRIDGE_FACTORY_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_factory_file)) && \
	OMNI_BRIDGE_ADDRESS=$$(jq -r .contract_id $(pipeline2_omni_bridge_contract_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$BRIDGE_FACTORY_ADDRESS \
		-m acl_grant_role \
		-a "{\"role\": \"Controller\", \"account_id\": \"$$OMNI_BRIDGE_ADDRESS\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: set-bridge-token-controller-omni-bridge
set-bridge-token-controller-omni-bridge: $(pipeline2_bridge_token_controller_omni_bridge_call_file)
$(pipeline2_bridge_token_controller_omni_bridge_call_file): $(pipeline2_bridge_factory_controller_grant_omni_bridge_call_file) $(pipeline2_old_bridge_token_deploy_file) $(pipeline2_omni_bridge_contract_file) $(near_init_account_credentials_file) | $(pipeline2_call_dir)
	$(call description,Granting controller role of bridge token to Omni Bridge)
	TOKEN_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_deploy_file)) && \
	OMNI_BRIDGE_ADDRESS=$$(jq -r .contract_id $(pipeline2_omni_bridge_contract_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$TOKEN_ADDRESS \
		-m set_controller	 \
		-a "{\"controller\": \"$$OMNI_BRIDGE_ADDRESS\"}" \
		-f $(near_init_account_credentials_file) \
		-g "300.0 Tgas" \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: add-token-to-omni-bridge
add-token-to-omni-bridge: $(pipeline2_add_token_to_omni_bridge_call_file)
$(pipeline2_add_token_to_omni_bridge_call_file): $(pipeline2_old_bridge_token_deploy_file) $(pipeline2_old_bridge_token_deploy_file) $(pipeline2_omni_bridge_dao_grant_call_file) $(pipeline2_omni_bridge_contract_file) $(sepolia_test_token_address_file) $(near_dao_account_credentials_file) | $(pipeline2_call_dir)
	$(call description,Adding token to Omni Bridge)
	TOKEN_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_deploy_file)) && \
	OMNI_BRIDGE_ADDRESS=$$(jq -r .contract_id $(pipeline2_omni_bridge_contract_file)) && \
	ETH_TOKEN_ADDRESS=eth:$$(jq -r .contractAddress $(sepolia_test_token_address_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$OMNI_BRIDGE_ADDRESS \
		-m add_deployed_tokens \
		-a "{\"tokens\": [{\"token_address\": \"$$ETH_TOKEN_ADDRESS\", \"token_id\": \"$$TOKEN_ADDRESS\", \"decimals\": 18}]}" \
		-f $(near_dao_account_credentials_file) \
		-d $(pipeline2_omni_bridge_add_token_deposit) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

.PHONY: mint-token-to-omni-bridge-eth
mint-token-to-omni-bridge-eth: $(pipeline2_mint_token_to_omni_bridge_eth_call_file)	
$(pipeline2_mint_token_to_omni_bridge_eth_call_file): $(pipeline2_eth_omni_bridge_contract_file) $(sepolia_test_token_address_file) | $(pipeline2_call_dir)
	$(call description,Minting token to Eth Omni Bridge)
	OMNI_BRIDGE_ADDRESS=$$(jq -r .bridgeAddress $(pipeline2_eth_omni_bridge_contract_file)) && \
	TOKEN_ADDRESS=$$(jq -r .contractAddress $(sepolia_test_token_address_file)) && \
	$(call EVM_MINT_TEST_TOKEN,sepolia,$$TOKEN_ADDRESS,$$OMNI_BRIDGE_ADDRESS,$(pipeline2_test_token_amount)) > $@


.PHONY: create-eth-user
create-eth-user: $(pipeline2_eth_user_account_file)
$(pipeline2_eth_user_account_file): | $(pipeline2_call_dir)
	$(call description,Creating EOA account)
	$(call EVM_CREATE_EOA,sepolia) > $@


.PHONY: omni-bridge-storage-deposit
omni-bridge-storage-deposit: $(pipeline2_omni_bridge_storage_deposit_call_file)
$(pipeline2_omni_bridge_storage_deposit_call_file): $(pipeline2_omni_bridge_contract_file) $(near_init_account_credentials_file) | $(pipeline2_call_dir)
	$(call description,Depositing storage for Omni Bridge)
	OMNI_BRIDGE_ADDRESS=$$(jq -r .contract_id $(pipeline2_omni_bridge_contract_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$OMNI_BRIDGE_ADDRESS \
		-m storage_deposit \
		-a "{\"account_id\": \"$$OMNI_BRIDGE_ADDRESS\"}" \
		-d "1 NEAR" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: withdraw-token-on-near
withdraw-token-on-near: $(pipeline2_withdraw_token_on_near_call_file)
$(pipeline2_withdraw_token_on_near_call_file): $(pipeline2_mint_token_to_omni_bridge_eth_call_file) $(pipeline2_omni_bridge_storage_deposit_call_file) $(pipeline2_mint_token_to_user_near_call_file) $(pipeline2_add_token_to_omni_bridge_call_file) $(pipeline2_bridge_token_controller_omni_bridge_call_file) $(pipeline2_old_bridge_token_deploy_file) $(pipeline2_user_account_file) $(pipeline2_eth_user_account_file) | $(pipeline2_call_dir)
	$(call description,Withdrawing token on NEAR)
	TOKEN_ADDRESS=$$(jq -r .contract_id $(pipeline2_old_bridge_token_deploy_file)) && \
	RECIPIENT_ADDRESS=$$(jq -r .address $(pipeline2_eth_user_account_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$TOKEN_ADDRESS \
		-m withdraw \
		-a "{\"amount\": \"$(pipeline2_test_token_amount)\", \"recipient\": \"$$RECIPIENT_ADDRESS\"}" \
		-f $(pipeline2_user_account_file) \
		-d "1 yoctoNEAR" \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: sign-withdraw-token-on-near
sign-withdraw-token-on-near: $(pipeline2_sign_withdraw_token_on_near_call_file)
$(pipeline2_sign_withdraw_token_on_near_call_file): $(pipeline2_withdraw_token_on_near_call_file) $(pipeline2_near_relayer_account_file) $(pipeline2_omni_bridge_contract_file) | $(pipeline2_call_dir)
	$(call description,Signing withdraw token on NEAR)
	OMNI_BRIDGE_ADDRESS=$$(jq -r .contract_id $(pipeline2_omni_bridge_contract_file)) && \
	TRANSFER_NONCE=1 && \
	RELAYER_ADDRESS=$$(jq -r .account_id $(pipeline2_near_relayer_account_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$OMNI_BRIDGE_ADDRESS \
		-m sign_transfer \
		-a "{\"transfer_id\": {\"origin_chain\": \"Near\", \"origin_nonce\": $$TRANSFER_NONCE}, \"fee_recipient\": \"$$RELAYER_ADDRESS\", \"fee\": {\"fee\": \"0\", \"native_fee\": \"0\"}}" \
		-f $(pipeline2_near_relayer_account_file) \
		-g "300.0 Tgas" \
		-d "1 yoctoNEAR" \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@

.PHONY: eth-fin-transfer
eth-fin-transfer: $(pipeline2_eth_fin_transfer_call_file)
$(pipeline2_eth_fin_transfer_call_file): $(pipeline2_sign_withdraw_token_on_near_call_file) $(pipeline2_eth_omni_bridge_contract_file) $(pipeline2_omni_bridge_contract_file) $(near_init_account_credentials_file)
	$(call description,Finilizing transfer on Ethereum)
	OMNI_BRIDGE_ADDRESS=$$(jq -r .contract_id $(pipeline2_omni_bridge_contract_file)) && \
	ETH_BRIDGE_TOKEN_FACTORY_ADDRESS=$$(jq -r .bridgeAddress $(pipeline2_eth_omni_bridge_contract_file)) && \
	TX_HASH=$$(jq -r .tx_hash $(pipeline2_sign_withdraw_token_on_near_call_file)) && \
	INIT_ACCOUNT_ID=$$(jq -r .account_id $(near_init_account_credentials_file)) && \
	bridge-cli testnet omni-connector evm-fin-transfer \
		--chain $(COMMON_SEPOLIA_CHAIN_STR) \
		--tx-hash $$TX_HASH \
		--near-token-locker-id $$OMNI_BRIDGE_ADDRESS \
		--eth-bridge-token-factory-address $$ETH_BRIDGE_TOKEN_FACTORY_ADDRESS \
		--near-signer $$INIT_ACCOUNT_ID \
		--config-file $(common_bridge_sdk_config_file) > $@ && \
	TX_HASH=$$(grep -o 'tx_hash="[^"]*"' $@ | cut -d'"' -f2) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: pipeline2-verify-correctness
pipeline2-verify-correctness: $(pipeline2_verify_correctness_report)
$(pipeline2_verify_correctness_report): $(pipeline2_eth_fin_transfer_call_file) $(sepolia_test_token_address_file) $(pipeline2_eth_user_account_file) $(common_tools_compile_stamp) | $(pipeline2_call_dir)
	$(call description, Waiting for Ethereum fin-transfer to be completed)
	$(call progress_wait,10)
	$(call description,Verifying correctness of the pipeline)
	TOKEN_ADDRESS=$$(jq -r .contractAddress $(sepolia_test_token_address_file)) && \
	RECIPIENT_ADDRESS=$$(jq -r .address $(pipeline2_eth_user_account_file)) && \
	yarn --cwd $(common_tools_dir) --silent verify-pipeline-2 \
		--tx-dir $(pipeline2_call_dir) \
		--token $$TOKEN_ADDRESS \
		--account $$RECIPIENT_ADDRESS \
		--balance $(pipeline2_test_token_amount) | tee $@
