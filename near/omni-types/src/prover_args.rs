use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};

use crate::prover_result::ProofKind;

pub type ProverId = String;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
pub struct VerifyProofArgs {
    pub prover_id: ProverId,
    pub prover_args: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug)]
pub struct EvmVerifyProofArgs {
    pub proof_kind: ProofKind,
    pub proof: EvmProof,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct WormholeVerifyProofArgs {
    pub proof_kind: ProofKind,
    pub vaa: String,
}

#[derive(Default, BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug)]
pub struct EvmProof {
    pub log_index: u64,
    pub log_entry_data: Vec<u8>,
    pub receipt_index: u64,
    pub receipt_data: Vec<u8>,
    pub header_data: Vec<u8>,
    pub proof: Vec<Vec<u8>>,
}
