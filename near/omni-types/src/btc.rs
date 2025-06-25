use bitcoin::{OutPoint, TxOut};
use near_sdk::serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum TokenReceiverMessage {
    DepositProtocolFee,
    Withdraw {
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    },
}
