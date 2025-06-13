use bitcoin::{OutPoint, TxOut};
use near_sdk::near;
use serde::{self, Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum TokenReceiverMessage {
    DepositProtocolFee,
    Withdraw {
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    },
}

#[near(serializers=[json])]
#[derive(Debug, Clone)]
pub struct UTXO {
    pub path: String,
    pub tx_bytes: Vec<u8>,
    pub vout: usize,
    #[serde(with = "u64_dec_format")]
    pub balance: u64,
}

pub mod u64_dec_format {
    use near_sdk::serde::de;
    use near_sdk::serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(num: &u64, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
    {
        serializer.serialize_str(&num.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
        where
            D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
