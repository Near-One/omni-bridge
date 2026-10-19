use near_sdk::json_types::U128;
use near_sdk::{near, AccountId};
use num_enum::IntoPrimitive;

use crate::{errors::BridgeError, ChainKind, Fee, Nonce, OmniAddress, TransferId};

/// Every prover result names the bridge contract that emitted the proven event.
/// The bridge selects a prover by chain, so the emitter must live on that chain;
/// otherwise a prover registered for one chain could attest events of another.
pub trait ProofMessage {
    fn emitter_address(&self) -> &OmniAddress;

    fn validate_chain(&self, chain_kind: ChainKind) -> Result<(), BridgeError> {
        if self.emitter_address().get_chain() == chain_kind {
            Ok(())
        } else {
            Err(BridgeError::ProofChainMismatch)
        }
    }
}

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

impl ProofMessage for InitTransferMessage {
    fn emitter_address(&self) -> &OmniAddress {
        &self.emitter_address
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct FinTransferMessage {
    pub transfer_id: TransferId,
    pub fee_recipient: Option<AccountId>,
    pub amount: U128,
    pub emitter_address: OmniAddress,
}

impl ProofMessage for FinTransferMessage {
    fn emitter_address(&self) -> &OmniAddress {
        &self.emitter_address
    }
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

impl ProofMessage for DeployTokenMessage {
    fn emitter_address(&self) -> &OmniAddress {
        &self.emitter_address
    }
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

impl ProofMessage for LogMetadataMessage {
    fn emitter_address(&self) -> &OmniAddress {
        &self.emitter_address
    }
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
#[derive(Debug, Clone, Copy, Eq, PartialEq, IntoPrimitive)]
#[repr(u8)]
pub enum ProofKind {
    InitTransfer,
    FinTransfer,
    DeployToken,
    LogMetadata,
}
