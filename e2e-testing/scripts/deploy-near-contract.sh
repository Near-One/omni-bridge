#!/usr/bin/env bash
set -euo pipefail

# Usage: ./deploy-near-contract.sh <wasm_path> <output_json> <contract_id> [<init_function> <init_args_json>]
# Example: ./deploy-near-contract.sh ./near_binary/omni_bridge.wasm ./near_deploy_results/omni_bridge.json omni-bridge-20240318-123456.testnet new '{"prover_account":"prover.sepolia.testnet"}'

if [ "$#" -lt 3 ]; then
    echo "Usage: $0 <wasm_path> <output_json> <contract_id> [<init_function> <init_args_json>]"
    exit 1
fi

WASM_PATH="$1"
OUTPUT_JSON="$2"
CONTRACT_ID="$3"
INIT_FUNCTION="${4:-}"
INIT_ARGS="${5:-}"
BINARY_NAME=$(basename "$WASM_PATH" .wasm)

echo "Deploying ${BINARY_NAME} to ${CONTRACT_ID}"

# Create the contract account
if ! near account create-account sponsor-by-faucet-service "$CONTRACT_ID" \
    autogenerate-new-keypair save-to-keychain network-config testnet create; then
    echo "Failed to create account for ${BINARY_NAME}"
    exit 1
fi

# Delay to allow the account to be created
sleep 3

# Deploy the contract
if [ -n "$INIT_FUNCTION" ] && [ -n "$INIT_ARGS" ]; then
    echo "Deploying with initialization: function=${INIT_FUNCTION}, args=${INIT_ARGS}"
    if ! near contract deploy "$CONTRACT_ID" use-file "$WASM_PATH" \
        with-init-call "$INIT_FUNCTION" json-args "$INIT_ARGS" \
        prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config testnet sign-with-keychain send; then
        echo "Failed to deploy ${BINARY_NAME} with initialization"
        exit 1
    fi
else
    echo "Deploying without initialization"
    if ! near contract deploy "$CONTRACT_ID" use-file "$WASM_PATH" \
        without-init-call network-config testnet sign-with-keychain send; then
        echo "Failed to deploy ${BINARY_NAME}"
        exit 1
    fi
fi

echo "{\"contract_id\": \"$CONTRACT_ID\"}" > "$OUTPUT_JSON"
echo "Deployment successful, saved to $OUTPUT_JSON" 