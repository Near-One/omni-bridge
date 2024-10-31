use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::AccountId;

use crate::{OmniAddress, TransferMessage};

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct InitTransferMessage {
    pub transfer: TransferMessage,
    pub emitter_address: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct FinTransferMessage {
    pub nonce: U128,
    pub fee_recipient: AccountId,
    pub amount: U128,
    pub emitter_address: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct DeployTokenMessage {
    pub token: AccountId,
    pub token_address: OmniAddress,
    pub emitter_address: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct TokenMetadataMessage {
    pub token_address: OmniAddress,
    pub token: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub emitter_address: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub enum ProverResult {
    InitTransfer(InitTransferMessage),
    FinTransfer(FinTransferMessage),
    DeployToken(DeployTokenMessage),
    TokenMetadata(TokenMetadataMessage),
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ProofKind {
    InitTransfer,
    FinTransfer,
    DeployToken,
}
