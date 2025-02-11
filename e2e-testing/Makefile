##########################################################
# Makefile prologue
##########################################################

MAKEFLAGS += --no-builtin-rules --no-builtin-variables --warn-undefined-variables --silent
unexport MAKEFLAGS
.DELETE_ON_ERROR:
.SUFFIXES:
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help
MAKEFILE_DIR := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

# Master clean target
.PHONY: clean
clean: clean-deploy-results clean-evm clean-near clean-solana clean-bridge-token-near-to-evm
	$(call description,Cleaning all build artifacts and deploy results)

# Include common module
include $(MAKEFILE_DIR)/makefiles/common.mk

# Include chain-specific modules
include $(MAKEFILE_DIR)/makefiles/evm.mk
include $(MAKEFILE_DIR)/makefiles/near.mk
include $(MAKEFILE_DIR)/makefiles/solana.mk

# Include test pipelines
include $(MAKEFILE_DIR)/makefiles/pipelines/bridge_token_near_to_evm.mk
include $(MAKEFILE_DIR)/makefiles/pipelines/02_bridge_migration.mk
# Help target
.PHONY: help
help:
	$(call description,Available targets)
	@echo "Build targets:"
	@echo "    evm-build                    Build EVM contracts"
	@echo "    evm-scripts-build            Build EVM deployment scripts"
	@echo "    near-build                   Build NEAR contracts"
	@echo "    solana-build                 Build Solana programs"
	@echo
	@echo "Clean targets:"
	@echo "    clean                             Clean all build artifacts and deploy results"
	@echo "    clean-deploy-results              Clean deploy results directories"
	@echo "    clean-evm                         Clean all EVM build artifacts"
	@echo "    clean-evm-{network}               Clean specific network deploy results"
	@echo "                                      Available networks: $(evm_networks)"
	@echo "    clean-near                        Clean NEAR build artifacts"
	@echo "    clean-solana                      Clean Solana build artifacts"
	@echo "    clean-bridge-token-near-to-evm    Clean bridge pipeline artifacts"
	@echo
	@echo "Account creation:"
	@echo "    create-near-init-account     Create NEAR initialization account"
	@echo "    create-near-sender           Create NEAR sender account"
	@echo "    create-near-relayer          Create NEAR relayer account"
	@echo "    create-dao-account           Create NEAR DAO account"
	@echo
	@echo "Deployment targets:"
	@echo "    near-deploy                  Deploy all NEAR contracts"
	@echo "    {network}-deploy             Deploy all contracts to specific EVM network"
	@echo "                                 Available networks: $(evm_networks)"
	@echo "    {network}-deploy-bridge      Deploy bridge contract to specific network"
	@echo "    {network}-deploy-enear       Deploy eNEAR token to specific network"
	@echo "    {network}-deploy-test-token  Deploy test token to specific network"
	@echo
	@echo "Bridge pipeline:"
	@echo "    bridge-token-near-to-evm             Run complete NEAR to ETH bridge test"
	@echo "    prepare-token-deployment             Prepare token deployment (Step 0)"
	@echo "    near-log-metadata-call               Log token metadata (Step 1)"
	@echo "    ethereum-deploy-token                Deploy token on Ethereum (Step 2)"
	@echo "    near-bind-token                      Bind token on NEAR (Step 3)"
	@echo "    verify-bridge-token-near-to-evm      Verify the correctness of deployment and binding"

