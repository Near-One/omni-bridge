use crate::ChainKind;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    AccountId,
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct StorageDepositArgs {
    pub token: AccountId,
    pub accounts: Vec<(AccountId, bool)>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct FinTransferArgs {
    pub chain_kind: ChainKind,
    pub storage_deposit_args: StorageDepositArgs,
    pub prover_args: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct ClaimFeeArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct BindTokenArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct DeployTokenArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}
