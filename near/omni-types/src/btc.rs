use near_sdk::json_types::{U128, Base64VecU8};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{near, AccountId};

type OutPoint = String;

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChainSpecificData {
    BtcData(ChainSpecificDataBtc),
    ZcashData(ChainSpecificDataZcash)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainSpecificDataBtc {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainSpecificDataZcash {
    pub orchard_bundle_bytes: Base64VecU8,
    pub expiry_height: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TokenReceiverMessage {
    DepositProtocolFee,
    Withdraw {
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        chain_specific_data: Option<ChainSpecificData>,
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


#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json;

    #[test]
    fn parses_withdraw_zcash_chain_specific_data_untagged() {
        let raw = r#"
        {
          "Withdraw": {
            "target_btc_address": "zs1",
            "input": [],
            "output": [],
            "max_gas_fee": "1000000000000000000",
            "chain_specific_data": {
              "orchard_bundle_bytes": "aGVsbG8=",
              "expiry_height": 2
            }
          }
        }
        "#;

        let msg: TokenReceiverMessage = serde_json::from_str(raw).unwrap();
        println!("{:?}", msg);

        match msg {
            TokenReceiverMessage::Withdraw {
                chain_specific_data, ..
            } => {
                let csd = chain_specific_data.expect("expected chain_specific_data");
                match csd {
                    ChainSpecificData::ZcashData(z) => {
                        assert_eq!(z.expiry_height, 2);
                        // "hello" bytes
                        assert_eq!(z.orchard_bundle_bytes.0, b"hello".to_vec());
                    }
                    ChainSpecificData::BtcData(_) => panic!("unexpectedly parsed as BTC"),
                }
            }
            _ => panic!("expected Withdraw message"),
        }
    }
}
