use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{near, AccountId};

#[derive(Debug, Serialize, Deserialize)]
pub enum TokenReceiverMessage {
    DepositProtocolFee,
    Withdraw {
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
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
pub struct OutPoint {
    pub txid: String,
    pub vout: u32,
}

#[near(serializers=[json])]
#[derive(Debug)]
pub struct TxOut {
    pub value: u64,
    pub script_pubkey: String,
}
