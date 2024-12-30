#!/bin/bash

usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  -c CONTRACT_ID       Contract ID (required)"
    echo "  -m METHOD_NAME       Method name to call (required)"
    echo "  -a METHOD_ARGS       Method arguments in JSON format (default: {})"
    echo "  -g GAS              Gas amount (default: 100.0 Tgas)"
    echo "  -d DEPOSIT          Deposit amount (default: 0 NEAR)"
    echo "  -f CREDENTIALS      Credentials JSON file with account_id, public_key, and private_key (required)"
    echo "  -n NETWORK          Network to use (default: testnet)"
    echo "  -h                  Show this help message"
    exit 1
}

# Default values
METHOD_ARGS="{}"
GAS="100.0 Tgas"
DEPOSIT="0 NEAR"
NETWORK="testnet"

# Parse command line arguments
while getopts "c:m:a:g:d:f:n:h" opt; do
    case $opt in
        c) CONTRACT_ID="$OPTARG" ;;
        m) METHOD_NAME="$OPTARG" ;;
        a) METHOD_ARGS="$OPTARG" ;;
        g) GAS="$OPTARG" ;;
        d) DEPOSIT="$OPTARG" ;;
        f) CREDENTIALS_FILE="$OPTARG" ;;
        n) NETWORK="$OPTARG" ;;
        h) usage ;;
        ?) usage ;;
    esac
done

# Validate required parameters
if [ -z "$CONTRACT_ID" ]; then
    echo "Error: CONTRACT_ID (-c) is required"
    usage
fi

if [ -z "$METHOD_NAME" ]; then
    echo "Error: METHOD_NAME (-m) is required"
    usage
fi

if [ -z "$CREDENTIALS_FILE" ]; then
    echo "Error: CREDENTIALS_FILE (-f) is required"
    usage
fi

if [ ! -f "$CREDENTIALS_FILE" ]; then
    echo "Error: Credentials file $CREDENTIALS_FILE does not exist"
    exit 1
fi

# Read credentials from file
SIGNER_ACCOUNT_ID=$(jq -r .account_id "$CREDENTIALS_FILE")
SIGNER_PUBLIC_KEY=$(jq -r .public_key "$CREDENTIALS_FILE")
SIGNER_PRIVATE_KEY=$(jq -r .private_key "$CREDENTIALS_FILE")

if [ -z "$SIGNER_ACCOUNT_ID" ] || [ "$SIGNER_ACCOUNT_ID" = "null" ]; then
    echo "Error: account_id not found in credentials file"
    exit 1
fi

if [ -z "$SIGNER_PUBLIC_KEY" ] || [ "$SIGNER_PUBLIC_KEY" = "null" ] || [ -z "$SIGNER_PRIVATE_KEY" ] || [ "$SIGNER_PRIVATE_KEY" = "null" ]; then
    echo "Error: public_key or private_key not found in credentials file"
    exit 1
fi

echo "Calling contract method: $METHOD_NAME"
if ! near contract call-function as-transaction "$CONTRACT_ID" \
    "$METHOD_NAME" \
    json-args "$METHOD_ARGS" \
    prepaid-gas "$GAS" attached-deposit "$DEPOSIT" \
    sign-as "$SIGNER_ACCOUNT_ID" \
    network-config "$NETWORK" \
    sign-with-plaintext-private-key --signer-public-key "$SIGNER_PUBLIC_KEY" --signer-private-key "$SIGNER_PRIVATE_KEY" \
    send; then
    echo "Failed to call method ${METHOD_NAME} on contract ${CONTRACT_ID}"
    exit 1
fi