use alloy_primitives::Log;
use alloy_rlp::Decodable;
use alloy_sol_types::{sol, SolEvent};

use crate::{
    prover_types::{DeployTokenMessage, FinTransferMessage, InitTransferMessage},
    stringify, OmniAddress, TransferMessage, H160,
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

pub fn parse_evm_event<T: SolEvent, V: TryFrom<Log<T>>>(log_rlp: Vec<u8>) -> Result<V, String>
where
    <V as TryFrom<Log<T>>>::Error: std::fmt::Display,
{
    let rlp_decoded = Log::decode(&mut log_rlp.as_slice()).map_err(stringify)?;
    V::try_from(T::decode_log(&rlp_decoded, true).map_err(stringify)?).map_err(stringify)
}

impl TryFrom<Log<FinTransfer>> for FinTransferMessage {
    type Error = String;

    fn try_from(event: Log<FinTransfer>) -> Result<Self, Self::Error> {
        Ok(FinTransferMessage {
            nonce: near_sdk::json_types::U128(event.data.nonce.to::<u128>()),
            claim_recipient: event.data.claim_recipient.parse().map_err(stringify)?,
            contract: OmniAddress::Eth(H160(event.address.into())),
        })
    }
}

impl TryFrom<Log<InitTransfer>> for InitTransferMessage {
    type Error = String;

    fn try_from(event: Log<InitTransfer>) -> Result<Self, Self::Error> {
        Ok(InitTransferMessage {
            contract: OmniAddress::Eth(H160(event.address.into())),
            transfer: TransferMessage {
                origin_nonce: near_sdk::json_types::U128(event.data.nonce.to::<u128>()),
                token: event.data.token.parse().map_err(stringify)?,
                amount: near_sdk::json_types::U128(event.data.amount.to::<u128>()),
                recipient: event.data.recipient.parse().map_err(stringify)?,
                fee: near_sdk::json_types::U128(event.data.fee.to::<u128>()),
                sender: OmniAddress::Eth(H160(event.data.sender.into())),
            },
        })
    }
}

impl TryFrom<Log<DeployToken>> for DeployTokenMessage {
    type Error = String;

    fn try_from(event: Log<DeployToken>) -> Result<Self, Self::Error> {
        Ok(DeployTokenMessage {
            contract: OmniAddress::Eth(H160(event.address.into())),
            token: event.data.token.parse().map_err(stringify)?,
            token_address: OmniAddress::Eth(H160(event.data.token_address.into())),
        })
    }
}
