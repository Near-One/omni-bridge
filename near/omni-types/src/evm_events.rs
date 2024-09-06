use alloy_primitives::Log;
use alloy_rlp::Decodable;
use alloy_sol_types::{sol, SolEvent};

use crate::{
    prover_result::{DeployTokenMessage, FinTransferMessage, InitTransferMessage},
    stringify, ChainKind, OmniAddress, TransferMessage, H160,
};

const ERR_INVALIDE_SIGNATURE_HASH: &str = "ERR_INVALIDE_SIGNATURE_HASH";

sol! {
    event FinTransfer(
        address indexed sender,
        uint nonce,
        uint amount,
        string claim_recipient,
    );

    event InitTransfer(
        address indexed sender,
        uint nonce,
        string token,
        uint amount,
        uint fee,
        string recipient,
    );

    event DeployToken(
        string token,
        address token_address,
    );
}

pub fn parse_evm_event<T: SolEvent, V: TryFromLog<Log<T>>>(
    chain_kind: ChainKind,
    log_rlp: Vec<u8>,
) -> Result<V, String>
where
    <V as TryFromLog<Log<T>>>::Error: std::fmt::Display,
{
    let rlp_decoded = Log::decode(&mut log_rlp.as_slice()).map_err(stringify)?;
    V::try_from_log(
        chain_kind,
        T::decode_log(&rlp_decoded, true).map_err(stringify)?,
    )
    .map_err(stringify)
}

pub trait TryFromLog<T>: Sized {
    type Error;
    fn try_from_log(chain_kind: ChainKind, value: T) -> Result<Self, Self::Error>;
}

impl TryFromLog<Log<FinTransfer>> for FinTransferMessage {
    type Error = String;

    fn try_from_log(chain_kind: ChainKind, event: Log<FinTransfer>) -> Result<Self, Self::Error> {
        if event.topics().0 != FinTransfer::SIGNATURE_HASH {
            return Err(ERR_INVALIDE_SIGNATURE_HASH.to_string());
        }

        Ok(FinTransferMessage {
            nonce: near_sdk::json_types::U128(event.data.nonce.to::<u128>()),
            amount: near_sdk::json_types::U128(event.data.nonce.to::<u128>()),
            fee_recipient: event.data.claim_recipient.parse().map_err(stringify)?,
            emitter_address: OmniAddress::from_evm_address(chain_kind, H160(event.address.into()))?,
        })
    }
}

impl TryFromLog<Log<InitTransfer>> for InitTransferMessage {
    type Error = String;

    fn try_from_log(chain_kind: ChainKind, event: Log<InitTransfer>) -> Result<Self, Self::Error> {
        if event.topics().0 != InitTransfer::SIGNATURE_HASH {
            return Err(ERR_INVALIDE_SIGNATURE_HASH.to_string());
        }

        Ok(InitTransferMessage {
            emitter_address: OmniAddress::from_evm_address(chain_kind, H160(event.address.into()))?,
            transfer: TransferMessage {
                origin_nonce: near_sdk::json_types::U128(event.data.nonce.to::<u128>()),
                token: event.data.token.parse().map_err(stringify)?,
                amount: near_sdk::json_types::U128(event.data.amount.to::<u128>()),
                recipient: event.data.recipient.parse().map_err(stringify)?,
                fee: near_sdk::json_types::U128(event.data.fee.to::<u128>()),
                sender: OmniAddress::from_evm_address(chain_kind, H160(event.data.sender.into()))?,
            },
        })
    }
}

impl TryFromLog<Log<DeployToken>> for DeployTokenMessage {
    type Error = String;

    fn try_from_log(chain_kind: ChainKind, event: Log<DeployToken>) -> Result<Self, Self::Error> {
        if event.topics().0 != DeployToken::SIGNATURE_HASH {
            return Err(ERR_INVALIDE_SIGNATURE_HASH.to_string());
        }

        Ok(DeployTokenMessage {
            emitter_address: OmniAddress::from_evm_address(chain_kind, H160(event.address.into()))?,
            token: event.data.token.parse().map_err(stringify)?,
            token_address: OmniAddress::from_evm_address(
                chain_kind,
                H160(event.data.token_address.into()),
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{IntoLogData, U256};

    use super::*;
    sol! {
        event TestFinTransfer(
            address indexed sender,
            uint nonce,
            string claim_recipient,
        );
    }

    #[test]
    fn test_decode_log_with_same_params_with_validation() {
        let event = FinTransfer {
            sender: [0; 20].into(),
            nonce: U256::from(55),
            claim_recipient: "some_claim_recipient".to_string(),
        };
        let test_event = TestFinTransfer {
            sender: event.sender,
            nonce: event.nonce,
            claim_recipient: event.claim_recipient.clone(),
        };
        let log = Log {
            address: [1; 20].into(),
            data: event.to_log_data(),
        };
        let test_log = Log {
            address: log.address,
            data: test_event.to_log_data(),
        };

        assert_ne!(log, test_log);

        let decoded_log = FinTransfer::decode_log(&log, true).unwrap();
        let decoded_test_log = TestFinTransfer::decode_log(&test_log, true).unwrap();

        assert_ne!(FinTransfer::SIGNATURE_HASH, TestFinTransfer::SIGNATURE_HASH);
        assert_eq!(FinTransfer::SIGNATURE_HASH, decoded_log.topics().0);
        assert_eq!(TestFinTransfer::SIGNATURE_HASH, decoded_test_log.topics().0);
    }
}
