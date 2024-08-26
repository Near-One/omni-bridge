use bridge_common::prover::{EthAddress, EthEvent, EthEventParams};
use ethabi::ParamType;
use hex::ToHex;
use near_contract_standards::fungible_token::Balance;

/// Data that was emitted by the Ethereum Unlocked event.
#[derive(Debug, Eq, PartialEq)]
pub struct TokenUnlockedEvent {
    pub eth_factory_address: EthAddress,
    pub token: String,
    pub sender: String,
    pub amount: Balance,
    pub recipient: String,
    pub token_eth_address: EthAddress,
}

impl TokenUnlockedEvent {
    fn event_params() -> EthEventParams {
        vec![
            ("token".to_string(), ParamType::String, false),
            ("sender".to_string(), ParamType::Address, true),
            ("amount".to_string(), ParamType::Uint(256), false),
            ("recipient".to_string(), ParamType::String, false),
            ("tokenEthAddress".to_string(), ParamType::Address, true),
        ]
    }

    pub fn from_log_entry_data(data: &[u8]) -> Self {
        let event =
            EthEvent::from_log_entry_data("Withdraw", TokenUnlockedEvent::event_params(), data);
        let token = event.log.params[0].value.clone().to_string().unwrap();
        let sender = event.log.params[1].value.clone().to_address().unwrap().0;
        let sender = (&sender).encode_hex::<String>();
        let amount = event.log.params[2]
            .value
            .clone()
            .to_uint()
            .unwrap()
            .as_u128();
        let recipient = event.log.params[3].value.clone().to_string().unwrap();
        let token_eth_address = event.log.params[4].value.clone().to_address().unwrap().0;
        Self {
            eth_factory_address: event.locker_address,
            token,
            sender,
            amount,
            recipient,
            token_eth_address,
        }
    }

    pub fn from_wormhole_payload(_data: &[u8]) -> Self {
        //TODO
        Self {
            eth_factory_address: [0u8; 20],
            token: "".to_string(),
            sender: "".to_string(),
            amount: 0,
            recipient: "".to_string(),
            token_eth_address: [0u8; 20]
        }
    }
}
