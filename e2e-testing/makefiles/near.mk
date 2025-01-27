# NEAR-specific variables and rules
near_dir := $(common_testing_root)/../near
near_binary_dir := $(common_generated_dir)/near_artifacts

near_init_params_file := $(common_testing_root)/near_init_params.json

# List all expected WASM binaries
near_binaries := evm_prover.wasm omni_bridge.wasm omni_prover.wasm token_deployer.wasm wormhole_omni_prover_proxy.wasm mock_token.wasm
near_binary_paths := $(addprefix $(near_binary_dir)/,$(near_binaries))

# List of binaries that require dynamic init args
near_binaries_with_dynamic_args := token_deployer mock_token omni_bridge

near_deploy_results := $(patsubst $(near_binary_dir)/%.wasm,$(common_near_deploy_results_dir)/%.json,$(near_binary_paths))

near_init_account_credentials_file := $(common_near_deploy_results_dir)/omni-init-account.json
near_dao_account_credentials_file := $(common_near_deploy_results_dir)/omni-dao-account.json

near_prover_dau_grant_call_file := $(common_near_deploy_results_dir)/omni-prover-dau-grant-call.json

near_evm_prover_setup_call_file := $(common_near_deploy_results_dir)/evm-prover-setup-call.json

# Clean targets
.PHONY: clean-near
clean-near:
	$(call description,Cleaning NEAR build artifacts)
	rm -rf $(near_binary_dir)
	rm -f $(common_testing_root)/*_dyn_init_args.json

# Main deployment targets
.PHONY: near-deploy
near-deploy: $(near_deploy_results)

.PHONY: near-build
near-build: $(near_binary_paths)

$(near_binary_paths) &:
	$(call description,Building NEAR contracts)
	$(near_dir)/build.sh --output-dir $(near_binary_dir)

# Account creation rules
.PHONY: create-near-init-account
create-near-init-account: $(near_init_account_credentials_file)
$(near_init_account_credentials_file): | $(common_near_deploy_results_dir)
	$(call description,Creating NEAR init account)
	$(common_scripts_dir)/create-near-account.sh omni-init-$(common_timestamp).testnet $@

.PHONY: create-dao-account
create-dao-account: $(near_dao_account_credentials_file)
$(near_dao_account_credentials_file): | $(common_near_deploy_results_dir)
	$(call description,Creating NEAR DAO account)
	$(common_scripts_dir)/create-near-account.sh omni-dao-$(common_timestamp).testnet $@

# Contract deployment rules
define generate_near_deploy_rules

$(1)_name := $$(basename $$(notdir $(1)))

# Check if the binary requires dynamic init args
ifeq ($$(filter $$($(1)_name),$(near_binaries_with_dynamic_args)),)

# Rule for binaries without dynamic init args
$(common_near_deploy_results_dir)/$$($(1)_name).json: $(near_init_params_file) $(near_init_account_credentials_file) $(1) | $(common_near_deploy_results_dir)
	$(call description,Deploying $$($(1)_name) contract)
	$(common_scripts_dir)/deploy-near-contract.sh $$^ $$($(1)_name)-$(common_timestamp).testnet $$@

else

# Rule for binaries with dynamic init args
$(common_near_deploy_results_dir)/$$($(1)_name).json: $(near_init_params_file) $(near_init_account_credentials_file) $(common_generated_dir)/$$($(1)_name)_dyn_init_args.json $(1) | $(common_near_deploy_results_dir)
	$(call description,Deploying $$($(1)_name) contract with dynamic init args)
	$(common_scripts_dir)/deploy-near-contract.sh $$^ $$($(1)_name)-$(common_timestamp).testnet $$@

endif

endef

$(foreach binary,$(near_binary_paths),$(eval $(call generate_near_deploy_rules,$(binary))))

# Dynamic init args generation
$(common_generated_dir)/token_deployer_dyn_init_args.json: $(common_near_deploy_results_dir)/omni_bridge.json $(near_init_account_credentials_file)
	$(call description,Generating token deployer init args)
	CONTROLLER_ADDRESS=$$(jq -r .contract_id $(common_near_deploy_results_dir)/omni_bridge.json) && \
	DAO_ADDRESS=$$(jq -r .account_id $(near_init_account_credentials_file)) && \
	echo "{\"controller\": \"$$CONTROLLER_ADDRESS\", \"dao\": \"$$DAO_ADDRESS\"}" > $@

$(common_generated_dir)/mock_token_dyn_init_args.json: $(near_init_account_credentials_file)
	$(call description,Generating mock token init args)
	OWNER_ADDRESS=$$(jq -r .account_id $<) && \
	echo "{\"owner_id\": \"$$OWNER_ADDRESS\"}" > $@

$(common_generated_dir)/omni_bridge_dyn_init_args.json: $(common_near_deploy_results_dir)/omni_prover.json
	$(call description,Generating omni bridge init args)
	PROVER_ADDRESS=$$(jq -r .contract_id $<) && \
	echo "{\"prover_account\": \"$$PROVER_ADDRESS\"}" > $@


.PHONY: omni-prover-dau-grant
omni-prover-dau-grant: $(near_prover_dau_grant_call_file)
$(near_prover_dau_grant_call_file): $(near_init_account_credentials_file) $(near_dao_account_credentials_file) $(common_near_deploy_results_dir)/omni_prover.json
	$(call description,Granting DAO role to omni prover)
	OMNI_PROVER_ACCOUNT_ID=$$(jq -r .contract_id $(common_near_deploy_results_dir)/omni_prover.json) && \
	DAO_ACCOUNT_ID=$$(jq -r .account_id $(near_dao_account_credentials_file)) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$OMNI_PROVER_ACCOUNT_ID \
		-m acl_grant_role \
		-a "{\"role\": \"DAO\", \"account_id\": \"$$DAO_ACCOUNT_ID\"}" \
		-f $(near_init_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@


.PHONY: evm-prover-setup
evm-prover-setup: $(near_evm_prover_setup_call_file)
$(near_evm_prover_setup_call_file): $(common_near_deploy_results_dir)/omni_prover.json $(common_near_deploy_results_dir)/evm_prover.json $(near_prover_dau_grant_call_file)
	$(call description,Setting up EVM prover)
	OMNI_PROVER_ACCOUNT_ID=$$(jq -r .contract_id $(common_near_deploy_results_dir)/omni_prover.json) && \
	EVM_PROVER_ACCOUNT_ID=$$(jq -r .contract_id $(common_near_deploy_results_dir)/evm_prover.json) && \
	$(common_scripts_dir)/call-near-contract.sh -c $$OMNI_PROVER_ACCOUNT_ID \
		-m add_prover \
		-a "{\"account_id\": \"$$EVM_PROVER_ACCOUNT_ID\", \"prover_id\": \"$(COMMON_SEPOLIA_CHAIN_STR)\"}" \
		-f $(near_dao_account_credentials_file) \
		-n testnet 2>&1 | tee $@ && \
	TX_HASH=$$(grep -o 'Transaction ID: [^ ]*' $@ | cut -d' ' -f3) && \
	echo "{\"tx_hash\": \"$$TX_HASH\"}" > $@ 