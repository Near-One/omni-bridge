use bitcoin::{OutPoint, TxOut};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

#[derive(Debug, Serialize, Deserialize)]
pub enum TokenReceiverMessage {
    DepositProtocolFee,
    Withdraw {
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    },
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UTXO {
    pub path: String,
    pub tx_bytes: Vec<u8>,
    pub vout: usize,
    #[serde_as(as = "DisplayFromStr")]
    pub balance: u64,
}
