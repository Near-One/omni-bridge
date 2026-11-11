use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::Value;
use near_sdk::{near, AccountId};

type OutPoint = String;

#[derive(Debug, Serialize, Deserialize)]
pub enum TokenReceiverMessage {
    DepositProtocolFee,
    Withdraw {
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        /// Passed through to the connector as is. For Zcash it holds the Orchard bundle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chain_specific_data: Option<Value>,
    },
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct UTXOChainConfig {
    pub connector: AccountId,
    pub token_id: AccountId,
}

#[near(serializers=[json])]
#[derive(Debug)]
pub struct TxOut {
    pub value: u64,
    pub script_pubkey: String,
}
