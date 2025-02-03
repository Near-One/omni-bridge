use near_sdk::near;

use crate::prover_result::ProofKind;

pub type ProverId = String;

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct VerifyProofArgs {
    pub prover_id: ProverId,
    pub prover_args: Vec<u8>,
}

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
#[derive(Default, Debug, Clone)]
pub struct EvmProof {
    pub log_index: u64,
    pub log_entry_data: Vec<u8>,
    pub receipt_index: u64,
    pub receipt_data: Vec<u8>,
    pub header_data: Vec<u8>,
    pub proof: Vec<Vec<u8>>,
}
