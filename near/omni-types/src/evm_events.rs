use alloy_primitives::Log;
use alloy_rlp::Decodable;
use alloy_sol_types::{sol, SolEvent};

use crate::{
    prover_types::{DeployTokenMessage, FinTransferMessage, InitTransferMessage},
    stringify, ChainKind, OmniAddress, TransferMessage, H160,
};

sol! {
    event FinTransfer(
        address indexed sender,
        uint nonce,
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
        Ok(FinTransferMessage {
            nonce: near_sdk::json_types::U128(event.data.nonce.to::<u128>()),
            claim_recipient: event.data.claim_recipient.parse().map_err(stringify)?,
            contract: OmniAddress::from_evm_address(chain_kind, H160(event.address.into()))?,
        })
    }
}

impl TryFromLog<Log<InitTransfer>> for InitTransferMessage {
    type Error = String;

    fn try_from_log(chain_kind: ChainKind, event: Log<InitTransfer>) -> Result<Self, Self::Error> {
        Ok(InitTransferMessage {
            contract: OmniAddress::from_evm_address(chain_kind, H160(event.address.into()))?,
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
        Ok(DeployTokenMessage {
            contract: OmniAddress::from_evm_address(chain_kind, H160(event.address.into()))?,
            token: event.data.token.parse().map_err(stringify)?,
            token_address: OmniAddress::from_evm_address(
                chain_kind,
                H160(event.data.token_address.into()),
            )?,
        })
    }
}
