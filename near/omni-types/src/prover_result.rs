use near_sdk::json_types::U128;
use near_sdk::{near, AccountId};

use crate::{Fee, Nonce, OmniAddress, TransferId};

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct InitTransferMessage {
    pub origin_nonce: Nonce,
    pub token: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
    pub msg: String,
    pub emitter_address: OmniAddress,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct FinTransferMessage {
    pub transfer_id: TransferId,
    pub fee_recipient: AccountId,
    pub amount: U128,
    pub emitter_address: OmniAddress,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct DeployTokenMessage {
    pub token: AccountId,
    pub token_address: OmniAddress,
    pub decimals: u8,
    pub origin_decimals: u8,
    pub emitter_address: OmniAddress,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct LogMetadataMessage {
    pub token_address: OmniAddress,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub emitter_address: OmniAddress,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub enum ProverResult {
    InitTransfer(InitTransferMessage),
    FinTransfer(FinTransferMessage),
    DeployToken(DeployTokenMessage),
    LogMetadata(LogMetadataMessage),
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProofKind {
    InitTransfer,
    FinTransfer,
    DeployToken,
    LogMetadata,
}
