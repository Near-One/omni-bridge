IFS=' ' read -r -a tokens <<< "$TOKENS_LIST"

for token in "${tokens[@]}"; do
    echo "Token: $token"
    balance=$(near contract call-function as-read-only $token ft_balance_of json-args "{\"account_id\": \"$TOKEN_LOCKER\"}" network-config testnet now)

    echo "Balance: $balance."
   
    near contract call-function as-transaction $token storage_deposit json-args "{\"account_id\": \"$OMNI_BRIDGE_NEAR\"}" prepaid-gas '100.0 Tgas' attached-deposit '0.00125 NEAR' sign-as $SIGNER_ACCOUNT_ID network-config $NEAR_NETWORK sign-with-keychain send

    ARGS=$(echo "{\"token_id\": \"$token\", \"receiver_id\": \"$OMNI_BRIDGE_NEAR\", \"amount\": $balance}" | base64 | tr -d '\n')

    near contract call-function as-transaction $DAO_ACCOUNT_ID add_proposal json-args "{\"proposal\": {\"description\": \"Transfer $token to OmniBridge\", \"kind\": { \"FunctionCall\": {\"receiver_id\": \"${TOKEN_LOCKER}\", \"actions\": [{\"method_name\": \"transfer_tokens\", \"args\": \"${ARGS}\", \"deposit\": \"1\", \"gas\": \"30000000000000\"}]}}}}" prepaid-gas '100.0 Tgas' attached-deposit '1 NEAR' sign-as $SIGNER_ACCOUNT_ID network-config $NEAR_NETWORK sign-with-keychain send

done
