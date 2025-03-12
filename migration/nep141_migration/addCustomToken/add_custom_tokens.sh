#!/bin/bash

source .env

IFS=' ' read -r -a tokens <<< "$TOKENS_LIST"

for near_token in "${tokens[@]}"; do
  echo "Near Token: $near_token"

  token_address=$(cast call $BRIDGE_TOKEN_FACTORY "nearToEthToken(string)(address)" "$near_token" --rpc-url $RPC_URL)
  echo "Token Ethereum Address: $token_address"

  if [[ -z "$token_address" || "$token_address" == "0x0000000000000000000000000000000000000000" ]]; then
    echo "No Address fetched for $near_token"
    continue
  fi

  decimals=$(cast call $token_address "decimals()(uint8)" --rpc-url $RPC_URL)
  echo "Decimals for token: $decimals"

  if [[ -z "$decimals" ]]; then
    echo "Error: no decimals found $token_address"
    continue
  fi

  cast send "$OMNI_BRIDGE_ETH" "addCustomToken(string,address,address,uint8)" \
    "$near_token" "$token_address" "0x0000000000000000000000000000000000000000" "$decimals" --private-key "$OMNI_BRIDGE_ADMIN_PRIVATE_KEY" --rpc-url $RPC_URL

done

