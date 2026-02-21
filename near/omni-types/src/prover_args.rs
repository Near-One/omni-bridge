use near_sdk::near;

use crate::{prover_result::ProofKind, H160};

#[near(serializers=[borsh])]
#[derive(Debug, Clone)]
pub struct EvmVerifyProofArgs {
    pub proof_kind: ProofKind,
    pub proof: EvmProof,
}

#[near(serializers=[borsh])]
#[derive(Debug, Clone)]
pub struct WormholeVerifyProofArgs {
    pub proof_kind: ProofKind,
    pub vaa: String,
}

#[near(serializers=[borsh])]
#[derive(Debug, Clone)]
pub struct HotVerifyProofArgs {
    pub proof_kind: ProofKind,
    pub signature: [u8; 64],
    pub transfer: HotInitTransfer,
}

#[near(serializers=[borsh])]
#[derive(Debug, Clone)]
pub struct HotInitTransfer {
    pub sender: H160,
    pub token_address: H160,
    pub origin_nonce: u64,
    pub amount: u128,
    pub fee: u128,
    pub native_fee: u128,
    pub recipient: String,
    pub message: String,
}

#[near(serializers=[borsh, json])]
#[derive(Default, Debug, Clone)]
pub struct EvmProof {
    pub log_index: u64,
    pub log_entry_data: Vec<u8>,
    pub receipt_index: u64,
    pub receipt_data: Vec<u8>,
    pub header_data: Vec<u8>,
    pub proof: Vec<Vec<u8>>,
}
