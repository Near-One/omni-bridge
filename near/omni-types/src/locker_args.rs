use crate::ChainKind;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct FinTransferArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}
