#!/usr/bin/env bash
set -euo pipefail

# Usage: ./deploy-near-contract.sh <wasm_path> <output_json> <contract_id> <near_init_params_file> <init_account_id> [<dyn_init_args_file>]

if [ "$#" -lt 5 ]; then
    echo "Usage: $0 <wasm_path> <output_json> <contract_id> <near_init_params_file> <init_account_id> [<dyn_init_args_file>]"
    exit 1
fi

WASM_PATH="$1"
OUTPUT_JSON="$2"
CONTRACT_ID="$3"
NEAR_INIT_PARAMS_FILE="$4"
INIT_ACCOUNT_CREDENTIALS_FILE="$5"
DYN_INIT_ARGS_FILE="${6:-}"

INIT_ACCOUNT_ID=$(jq -r .account_id "$INIT_ACCOUNT_CREDENTIALS_FILE")
INIT_ACCOUNT_PUBLIC_KEY=$(jq -r .public_key "$INIT_ACCOUNT_CREDENTIALS_FILE")
INIT_ACCOUNT_PRIVATE_KEY=$(jq -r .private_key "$INIT_ACCOUNT_CREDENTIALS_FILE")

CONTRACT_NAME=$(basename "$2" .json)


# Extract init function and merge init args
INIT_FUNCTION=$(jq -rc ".$CONTRACT_NAME.init_function // \"\"" "$NEAR_INIT_PARAMS_FILE")
STATIC_INIT_ARGS=$(jq -rc ".$CONTRACT_NAME.init_args // {}" "$NEAR_INIT_PARAMS_FILE")

if [ -f "$DYN_INIT_ARGS_FILE" ]; then
    DYN_INIT_ARGS=$(cat "$DYN_INIT_ARGS_FILE")
else
    DYN_INIT_ARGS="{}"
fi

INIT_ARGS=$(echo "$STATIC_INIT_ARGS $DYN_INIT_ARGS" | jq -s add)

echo "Creating the contract account"
# Create the contract account
if ! near account create-account sponsor-by-faucet-service "$CONTRACT_ID" \
    autogenerate-new-keypair save-to-keychain network-config testnet create; then
    echo "Failed to create account for ${CONTRACT_NAME}"
    exit 1
fi

# Delay to allow the account to be created
sleep 3

# Deploy the contract 
echo "Deploying the contract"
if ! near contract deploy "$CONTRACT_ID" use-file "$WASM_PATH" \
    without-init-call network-config testnet sign-with-keychain send; then
    echo "Failed to deploy ${CONTRACT_NAME}"
    exit 1
fi

# Delay to allow the account to be deployed
sleep 3

# Init the contract
echo "Init the contract"
if ! near contract call-function as-transaction "$CONTRACT_ID" \
    "$INIT_FUNCTION" \
    json-args "$INIT_ARGS" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    sign-as "$INIT_ACCOUNT_ID" \
    network-config testnet \
    sign-with-plaintext-private-key --signer-public-key "$INIT_ACCOUNT_PUBLIC_KEY" --signer-private-key "$INIT_ACCOUNT_PRIVATE_KEY" \
    send; then
    echo "Failed to init ${CONTRACT_NAME}"
    exit 1
fi

echo "{\"contract_id\": \"$CONTRACT_ID\"}" > "$OUTPUT_JSON"
echo "Deployment successful, saved to $OUTPUT_JSON" 