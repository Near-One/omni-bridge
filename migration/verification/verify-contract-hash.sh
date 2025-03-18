#!/bin/bash

# Usage:
#   ./compare_hashes.sh <wasm_file> <account_id>
#
#   <wasm_file>   - Path to the WASM file.
#   <account_id>  - NEAR account ID to query.
#                   Must end with ".near" for mainnet or ".testnet" for testnet.

# Check if the required arguments are provided
if [ -z "$1" ] || [ -z "$2" ]; then
    echo "Usage: $0 <wasm_file> <account_id>"
    exit 1
fi

WASM_FILE="$1"
ACCOUNT_ID="$2"

# Determine network based on account id suffix
if [[ "$ACCOUNT_ID" == *.near ]]; then
    RPC_URL="https://rpc.mainnet.near.org"
elif [[ "$ACCOUNT_ID" == *.testnet ]]; then
    RPC_URL="https://rpc.testnet.near.org"
else
    echo "Invalid account id. It must end with .near or .testnet"
    exit 1
fi

echo "Using account id: $ACCOUNT_ID"
echo "Using network RPC: $RPC_URL"
echo "Using WASM file: $WASM_FILE"

# Check if WASM file exists
if [ ! -f "$WASM_FILE" ]; then
    echo "File not found: $WASM_FILE"
    exit 1
fi

# Compute SHA-256 checksum in hexadecimal for the WASM file
sha_hex=$(sha256sum "$WASM_FILE" | awk '{print $1}')
echo "SHA-256 checksum hex: $sha_hex"

# Compute Base58 (bs58) encoding of the SHA-256 checksum using Python
local_bs58=$(python3 - <<EOF
import hashlib
import base58
with open("$WASM_FILE", "rb") as f:
    data = f.read()
hash_bytes = hashlib.sha256(data).digest()
print(base58.b58encode(hash_bytes).decode())
EOF
)
echo "Local SHA-256 checksum bs58: $local_bs58"

# Prepare JSON payload for the NEAR RPC query
read -r -d '' JSON_PAYLOAD <<EOF
{
  "jsonrpc": "2.0",
  "id": "dontcare",
  "method": "query",
  "params": {
    "request_type": "view_account",
    "finality": "final",
    "account_id": "$ACCOUNT_ID"
  }
}
EOF

# Query the NEAR RPC endpoint and extract the code_hash using jq
remote_bs58=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" -d "$JSON_PAYLOAD" | jq -r '.result.code_hash')

if [ "$remote_bs58" = "null" ] || [ -z "$remote_bs58" ]; then
    echo "Failed to retrieve code_hash from $RPC_URL for account $ACCOUNT_ID."
    exit 1
fi

echo "Remote code_hash from $ACCOUNT_ID: $remote_bs58"

# Compare the local bs58 hash with the remote code_hash
if [ "$local_bs58" = "$remote_bs58" ]; then
    echo "Hashes match."
else
    echo "Hashes do NOT match."
fi
