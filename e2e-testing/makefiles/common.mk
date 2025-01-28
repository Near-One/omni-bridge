# Common variables and settings
common_testing_root := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))/..
common_timestamp := $(shell date -u +%Y%m%d-%H%M%S)
common_generated_dir := $(common_testing_root)/generated

# ASCII box formatting for step descriptions
define description
	@echo "|──────────────────────────────────────────────────────────────"
	@echo "│ $(1)"
	@echo "|──────────────────────────────────────────────────────────────"
endef

# Progress bar for waiting operations
define progress_wait
	@for i in $$(seq 1 $(1)); do \
		printf "\rProgress: [%-$(1)s] %d%%" "$$(printf '#%.0s' $$(seq 1 $$i))" "$$((i * 100 / $(1)))"; \
		sleep 1; \
	done; \
	printf '\n'
endef

# Common directories
common_near_deploy_results_dir := $(common_generated_dir)/near_deploy_results
common_evm_deploy_results_dir := $(common_generated_dir)/evm_deploy_results
common_solana_deploy_results_dir := $(common_generated_dir)/solana_deploy_results
common_tools_dir := $(common_testing_root)/tools
common_scripts_dir := $(common_tools_dir)/src/scripts

# Common files
common_near_bridge_id_file := $(common_near_deploy_results_dir)/omni_bridge.json
common_bridge_sdk_config_file := $(common_testing_root)/bridge-sdk-config.json
common_tools_compile_stamp := $(common_generated_dir)/.tools-compile.stamp

# Chain identifiers
COMMON_SEPOLIA_CHAIN_ID := 0
COMMON_SEPOLIA_CHAIN_STR := Eth
COMMON_NEAR_CHAIN_STR := Near

# Create required directories
$(common_generated_dir) $(common_near_deploy_results_dir) $(common_evm_deploy_results_dir) $(common_solana_deploy_results_dir):
	$(call description,Creating directory to store generated files: $@)
	mkdir -p $@

# Build tools
.PHONY: tools-build
tools-build: $(common_tools_compile_stamp)
$(common_tools_compile_stamp):
	$(call description,Building tools)
	yarn --cwd $(common_tools_dir) install && \
	yarn --cwd $(common_tools_dir) hardhat compile
	touch $@


# Clean targets
.PHONY: clean-deploy-results
clean-deploy-results:
	$(call description,Cleaning deploy results directories)
	rm -rf $(common_near_deploy_results_dir)
	rm -rf $(common_evm_deploy_results_dir)
	rm -rf $(common_solana_deploy_results_dir) 
	rm -rf $(common_tools_compile_stamp)