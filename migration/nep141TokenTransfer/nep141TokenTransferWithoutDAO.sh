source .env

IFS=' ' read -r -a tokens <<< "$TOKENS_LIST"

for token in "${tokens[@]}"; do
    echo "Token: $token"
    balance=$(near contract call-function as-read-only $token ft_balance_of json-args "{\"account_id\": \"$TOKEN_LOCKER\"}" network-config testnet now)

    echo "Balance: $balance."
   
    near contract call-function as-transaction $token storage_deposit json-args "{\"account_id\": \"$OMNI_BRIDGE_NEAR\"}" prepaid-gas '100.0 Tgas' attached-deposit '0.00125 NEAR' sign-as $TOKEN_LOCKER network-config $NEAR_NETWORK sign-with-keychain send

    near contract call-function as-transaction $token ft_transfer json-args "{\"receiver_id\": \"$OMNI_BRIDGE_NEAR\", \"amount\": $balance}" prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $TOKEN_LOCKER network-config $NEAR_NETWORK sign-with-keychain send

done
