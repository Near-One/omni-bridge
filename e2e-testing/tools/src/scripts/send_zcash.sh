#!/usr/bin/env bash
set -euo pipefail

set -a
source "$PWD/tools/.env"
set +a

ZCASH_ADDRESS="$1"
zingo-cli -c testnet --server https://testnet.zec.rocks:443 --data-dir $ZCASH_DATA_DIR quicksend $ZCASH_ADDRESS 25000

sleep 120
