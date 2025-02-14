#!/bin/bash

REDIS_HOST="127.0.0.1"
REDIS_PORT="6379"

get_last_processed() {
    local chain=$1
    local key=$2
    local value=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" get "$key")

    if [[ -n "$value" ]]; then
        echo "Last processed block on $chain: $value"
    else
        echo "No data found for $chain ($key)"
    fi
}

case "$1" in
  get_last_processed)
	get_last_processed "Near" "Near_LAST_PROCESSED_BLOCK"
	get_last_processed "Ethereum" "Eth_LAST_PROCESSED_BLOCK"
	get_last_processed "Base" "Base_LAST_PROCESSED_BLOCK"
	get_last_processed "Arbitrum" "Arb_LAST_PROCESSED_BLOCK"
	get_last_processed "Solana" "SOLANA_LAST_PROCESSED_SIGNATURE"
    ;;
  get_near_last_processed_block)
	get_last_processed "Near" "Near_LAST_PROCESSED_BLOCK"
    ;;
  get_eth_last_processed_block)
	get_last_processed "Ethereum" "Eth_LAST_PROCESSED_BLOCK"
    ;;
  get_base_last_processed_block)
	get_last_processed "Base" "Base_LAST_PROCESSED_BLOCK"
    ;;
  get_arb_last_processed_block)
	get_last_processed "Arbitrum" "Arb_LAST_PROCESSED_BLOCK"
    ;;
  get_evm_last_processed_block)
	get_last_processed "Ethereum" "Eth_LAST_PROCESSED_BLOCK"
	get_last_processed "Base" "Base_LAST_PROCESSED_BLOCK"
	get_last_processed "Arbitrum" "Arb_LAST_PROCESSED_BLOCK"
    ;;
  get_solana_last_processed_signature)
	get_last_processed "Solana" "SOLANA_LAST_PROCESSED_SIGNATURE"
    ;;
  get_events)
	redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" hgetall "events" | sed -n 'n;p' | sed G
    ;;
  get_solana_events)
	redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" hgetall "solana_events" | sed -n '1~2p'
    ;;
  get_stuck_transfers)
	redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" hgetall "stuck_transfers" | sed -n 'n;p' | sed G
    ;;
  *)
    echo "Unknown command: $1"
    echo
    echo "Usage: $0 <command>"
    echo
    echo "Available commands:"
    echo "  get_last_processed                  - Retrieve last processed blocks/signatures for all chains"
    echo "  get_near_last_processed_block       - Retrieve the last processed block for Near"
    echo "  get_eth_last_processed_block        - Retrieve the last processed block for Ethereum"
    echo "  get_base_last_processed_block       - Retrieve the last processed block for Base"
    echo "  get_arb_last_processed_block        - Retrieve the last processed block for Arbitrum"
    echo "  get_evm_last_processed_block        - Retrieve the last processed blocks for all EVM chains"
    echo "  get_solana_last_processed_signature - Retrieve the last processed signature for Solana"
    echo "  get_events                          - Retrieve all events related to Near"
    echo "  get_solana_events                   - Retrieve all event keys for Solana"
    echo "  get_stuck_transfers                 - Retrieve stuck transfer events"
    exit 1
    ;;
esac
