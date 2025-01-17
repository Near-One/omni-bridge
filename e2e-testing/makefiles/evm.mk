# EVM-specific variables and rules
evm_dir := $(common_testing_root)/../evm
evm_script_dir := $(common_testing_root)/evm_scripts

evm_compile_stamp := $(common_testing_root)/.evm-compile.stamp
evm_artifacts_dir := $(common_testing_root)/evm_artifacts
evm_script_compile_stamp := $(common_testing_root)/.evm-scripts-compile.stamp

evm_networks := sepolia arbitrumSepolia baseSepolia

# EVM deployment commands
EVM_DEPLOY_TOKEN_IMPL = yarn --silent --cwd $(evm_dir) hardhat deploy-token-impl --network $(1)
EVM_DEPLOY_OMNI_BRIDGE_CONTRACT = yarn --silent --cwd $(evm_dir) hardhat deploy-bridge-token-factory --network $(1) --bridge-token-impl $(2) --near-bridge-account-id $(3)
EVM_DEPLOY_FAKE_PROVER = yarn --silent --cwd $(evm_dir) hardhat deploy-fake-prover --network $(1)
EVM_DEPLOY_ENEAR_PROXY = yarn --silent --cwd $(evm_dir) hardhat deploy-e-near-proxy --network $(1) --enear $(2)

EVM_DEPLOY_BYTECODE = yarn --silent --cwd $(evm_script_dir) hardhat deploy-bytecode --network $(1) --bytecode $(2)
EVM_DEPLOY_TEST_TOKEN = yarn --silent --cwd $(evm_script_dir) hardhat deploy-test-token --network $(1) --name $(2) --symbol $(3)

evm_enear_creation_template_file := $(common_testing_root)/bin/eNear_creation.template

# Clean targets
.PHONY: clean-evm
clean-evm:
	$(call description,Cleaning EVM build artifacts)
	rm -rf $(evm_artifacts_dir)
	rm -f $(evm_compile_stamp)
	rm -f $(evm_script_compile_stamp)

# Build rules
.PHONY: evm-build
evm-build: $(evm_compile_stamp)
$(evm_compile_stamp):
	$(call description,Building EVM contracts)
	mkdir -p $(evm_artifacts_dir) && \
	yarn --cwd $(evm_dir) install --frozen-lockfile && \
	yarn --cwd $(evm_dir) hardhat compile && \
	cp -r $(evm_dir)/build/* $(evm_artifacts_dir)
	touch $@

.PHONY: evm-scripts-build
evm-scripts-build: $(evm_script_compile_stamp)
$(evm_script_compile_stamp):
	$(call description,Building EVM scripts)
	yarn --cwd $(evm_script_dir) install && \
	yarn --cwd $(evm_script_dir) hardhat compile
	touch $@

# Network-specific deployment rules
define generate_evm_deploy_rules

$(1)_deploy_results_dir := $(common_evm_deploy_results_dir)/$(1)

.PHONY: clean-evm-$(1)
clean-evm-$(1):
	$(call description,Cleaning EVM deploy results for $(1))
	rm -rf $$($(1)_deploy_results_dir)

$$($(1)_deploy_results_dir): | $(common_evm_deploy_results_dir)
	$(call description,Creating deploy results directory for $(1))
	mkdir -p $$@

$(1)_bridge_contract_address_file := $$($(1)_deploy_results_dir)/omni_bridge.json
$(1)_token_impl_address_file := $$($(1)_deploy_results_dir)/token_impl.json
$(1)_fake_prover_address_file := $$($(1)_deploy_results_dir)/fake_prover.json
$(1)_enear_address_file := $$($(1)_deploy_results_dir)/eNear.json
$(1)_enear_proxy_address_file := $$($(1)_deploy_results_dir)/eNearProxy.json
$(1)_enear_creation_file := $$($(1)_deploy_results_dir)/eNear_creation
$(1)_test_token_address_file := $$($(1)_deploy_results_dir)/test_token.json

.PHONY: $(1)-deploy-fake-prover
$(1)-deploy-fake-prover: $$($(1)_fake_prover_address_file)
$$($(1)_fake_prover_address_file): $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$(call description,Deploying fake prover to $(1))
	$$(call EVM_DEPLOY_FAKE_PROVER,$(1)) 2>/dev/stderr 1> $$@

$$($(1)_enear_creation_file): $$($(1)_fake_prover_address_file) $(evm_enear_creation_template_file) | $$($(1)_deploy_results_dir)
	$(call description,Creating eNear creation file for $(1))
	cat $$< | \
	sed "s/<PROVER_ADDRESS>/$$(shell cat $$< | jq -r .fakeProverAddress | sed 's/^0x//')/" > $$@

.PHONY: $(1)-deploy-enear
$(1)-deploy-enear: $$($(1)_enear_address_file)
$$($(1)_enear_address_file): $$($(1)_enear_creation_file) $(evm_script_compile_stamp) | $$($(1)_deploy_results_dir)
	$(call description,Deploying eNear to $(1))
	$$(call EVM_DEPLOY_BYTECODE,$(1),$$<) 2>/dev/stderr 1> $$@

.PHONY: $(1)-deploy-enear-proxy
$(1)-deploy-enear-proxy: $$($(1)_enear_proxy_address_file)
$$($(1)_enear_proxy_address_file): $$($(1)_enear_address_file) $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$(call description,Deploying eNear proxy to $(1))
	$$(call EVM_DEPLOY_ENEAR_PROXY,$(1),$$(shell cat $$< | jq -r .contractAddress)) 2>/dev/stderr 1> $$@

.PHONY: $(1)-deploy-token-impl
$(1)-deploy-token-impl: $$($(1)_token_impl_address_file)
$$($(1)_token_impl_address_file): $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$(call description,Deploying token implementation to $(1))
	$$(call EVM_DEPLOY_TOKEN_IMPL,$(1)) 2>/dev/stderr 1> $$@

.PHONY: $(1)-deploy-bridge
$(1)-deploy-bridge: $$($(1)_bridge_contract_address_file)
$$($(1)_bridge_contract_address_file): $$($(1)_token_impl_address_file) $(common_near_bridge_id_file) $(evm_compile_stamp) | $$($(1)_deploy_results_dir)
	$(call description,Deploying bridge contract to $(1))
	$$(call EVM_DEPLOY_OMNI_BRIDGE_CONTRACT,$(1),$$(shell cat $$< | jq -r .tokenImplAddress),$$(shell cat $(common_near_bridge_id_file) | jq -r .contract_id)) 2>/dev/stderr 1> $$@

.PHONY: $(1)-deploy-test-token
$(1)-deploy-test-token: $$($(1)_test_token_address_file)
$$($(1)_test_token_address_file): $(evm_script_compile_stamp) | $$($(1)_deploy_results_dir)
	$(call description,Deploying test token to $(1))
	$$(call EVM_DEPLOY_TEST_TOKEN,$(1),E2ETestToken-$(common_timestamp),E2ETT-$(common_timestamp)) 2>/dev/stderr 1> $$@

.PHONY: $(1)-deploy
$(1)-deploy: $(1)-deploy-bridge $(1)-deploy-enear-proxy $(1)-deploy-test-token

endef

$(foreach network,$(evm_networks),$(eval $(call generate_evm_deploy_rules,$(network)))) 