use near_sdk::near;

use crate::prover_result::ProofKind;

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
pub struct PolymerVerifyProofArgs {
    pub proof_kind: ProofKind,
    pub proof: Vec<u8>,
    pub src_chain_id: u64,
    pub src_block_number: u64,
    pub global_log_index: u64,
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
