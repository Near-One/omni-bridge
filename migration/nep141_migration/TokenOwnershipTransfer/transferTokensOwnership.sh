#!/bin/bash

# Load environment variables from .env
source .env

# Split the TOKENS_LIST into an array
IFS=' ' read -ra TOKENS <<< "$TOKENS_LIST"

# Function to call a smart contract method
call_contract() {
    local CONTRACT=$1
    local FUNC_SIG=$2
    local ARGS=$3
    local PRIVATE_KEY=$4

    echo "Calling contract: $CONTRACT, function: $FUNC_SIG, arguments: $ARGS"

    cast send "$CONTRACT" "$FUNC_SIG" $ARGS --private-key "$PRIVATE_KEY" --rpc-url "$RPC_URL"
    if [[ $? -ne 0 ]]; then
        echo "Error calling contract $CONTRACT"
        exit 1
    fi
}

# Iterate over each token and execute contract calls
for TOKEN in "${TOKENS[@]}"; do
    echo "Processing token: $TOKEN"

    # 1. Call tokenOwnerTransfer in BRIDGE_TOKEN_FACTORY
    call_contract "$BRIDGE_TOKEN_FACTORY" \
                  "tokenOwnerTransfer(string,address)" \
                  "$TOKEN $OMNI_BRIDGE_ETH" \
                  "$BRIDGE_TOKEN_FACTORY_ADMIN_PRIVATE_KEY"

    sleep 5 # Wait to ensure transaction confirmation

    ETH_TOKEN_ADDRESS=$(cast call "$BRIDGE_TOKEN_FACTORY" \
        "nearToEthToken(string)(address)" \
        "$TOKEN" \
        --rpc-url "$RPC_URL")

    # 2. Call acceptTokenOwnership in OMNI_BRIDGE_ETH
    call_contract "$OMNI_BRIDGE_ETH" \
                  "acceptTokenOwnership(address)" \
                  "$ETH_TOKEN_ADDRESS" \
                  "$OMNI_BRIDGE_ADMIN_PRIVATE_KEY"

    echo "Token $TOKEN processed successfully"
done

echo "All tokens have been successfully processed!"
