use crate::ChainKind;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    AccountId,
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct StorageDepositAction {
    pub token_id: AccountId,
    pub account_id: AccountId,
    pub storage_deposit_amount: Option<u128>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct FinTransferArgs {
    pub chain_kind: ChainKind,
    pub storage_deposit_actions: Vec<StorageDepositAction>,
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
