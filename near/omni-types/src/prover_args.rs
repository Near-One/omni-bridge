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
pub struct MpcVerifyProofArgs {
    pub proof_kind: ProofKind,
    pub sign_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub signature_big_r: String,
    pub signature_s: String,
    pub signature_recovery_id: u8,
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
