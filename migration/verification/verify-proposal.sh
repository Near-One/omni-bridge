DAO_ACCOUNT_ID="rainbowbridge.sputnik-dao.near"

read -p "Enter proposal Id: " proposal_id

near contract call-function as-read-only $DAO_ACCOUNT_ID get_proposal json-args "{\"id\": $proposal_id}" network-config mainnet now

read -p "Type 'approve' to approve the proposal: " approve

if [ "$approve" = "approve" ]; then
    near contract call-function as-transaction $DAO_ACCOUNT_ID act_proposal json-args "{\"id\": ${proposal_id}, \"action\": \"VoteApprove\"}" prepaid-gas '300 TeraGas' attached-deposit '0 NEAR' sign-as $SIGNER_ACCOUNT_ID network-config mainnet sign-with-ledger send
fi

