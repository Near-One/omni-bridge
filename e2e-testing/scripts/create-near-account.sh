#!/usr/bin/env bash
set -euo pipefail

ACCOUNT_ID="$1"
OUTPUT_JSON="$2"

echo "Creating account for ${ACCOUNT_ID}"

if ! near account create-account sponsor-by-faucet-service "$ACCOUNT_ID" \
    autogenerate-new-keypair save-to-legacy-keychain network-config testnet create; then
    echo "Failed to create account for ${ACCOUNT_ID}"
    exit 1
fi

# Extract private key from credentials
CREDENTIALS_FILE="$HOME/.near-credentials/testnet/$ACCOUNT_ID.json"
jq -c --arg account_id "$ACCOUNT_ID" '. + {account_id: $account_id}' "$CREDENTIALS_FILE" > "$OUTPUT_JSON"

echo "Account created successfully, saved to $OUTPUT_JSON"
