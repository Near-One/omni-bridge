.PHONY: ethereum-deploy evm-compile deploy-eth-bridge deploy-eth-token-impl create-near-bridge-id

ETHEREUM_TESTNET := sepolia
EVM_DIR := ../evm

# File outputs
NEAR_BRIDGE_ID_FILE := ./near-bridge-account-id
BRIDGE_CONTRACT_ADDRESS_FILE := ./ethereum-omni-bridge-contract-address
TOKEN_IMPL_ADDRESS_FILE := ./ethereum-token-impl-address
EVM_COMPILE_STAMP := ./.evm-compile.stamp

# Commands
DEPLOY_EVM_TOKEN_IMPL := yarn --silent --cwd $(EVM_DIR) hardhat deploy-token-impl --network $(1)
DEPLOY_EVM_OMNI_BRIDGE_CONTRACT := yarn --silent --cwd $(EVM_DIR) hardhat deploy-bridge-token-factory --network $(1) --bridge-token-impl $(2) --near-bridge-account-id $(3)

ethereum-deploy: deploy-eth-bridge

create-near-bridge-id: $(NEAR_BRIDGE_ID_FILE)
$(NEAR_BRIDGE_ID_FILE):
	@echo "STUB: Enter the NEAR bridge account ID"
	echo "eeeeeeeeetttttttttt.testnet" > $@

deploy-eth-bridge: $(BRIDGE_CONTRACT_ADDRESS_FILE)
$(BRIDGE_CONTRACT_ADDRESS_FILE): $(TOKEN_IMPL_ADDRESS_FILE) $(NEAR_BRIDGE_ID_FILE)
	@echo "Deploying EVM OmniBridge contract"
	$(call DEPLOY_EVM_OMNI_BRIDGE_CONTRACT,$(ETHEREUM_TESTNET),$(shell cat $(TOKEN_IMPL_ADDRESS_FILE) | jq -r .tokenImplAddress),$(shell cat $(NEAR_BRIDGE_ID_FILE))) 2>/dev/stderr 1> $@

deploy-eth-token-impl: $(TOKEN_IMPL_ADDRESS_FILE)
$(TOKEN_IMPL_ADDRESS_FILE): $(EVM_COMPILE_STAMP)
	@echo "Deploying EVM token implementation"
	$(call DEPLOY_EVM_TOKEN_IMPL,$(ETHEREUM_TESTNET)) 2>/dev/stderr 1> $@

evm-compile: $(EVM_COMPILE_STAMP)
$(EVM_COMPILE_STAMP):
	@echo "Compiling EVM contracts"
	yarn --cwd $(EVM_DIR) install && \
	yarn --cwd $(EVM_DIR) hardhat compile
	touch $@

