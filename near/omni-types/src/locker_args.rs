use crate::{ChainKind, OmniAddress};
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
    pub native_fee_recipient: OmniAddress,
    pub storage_deposit_args: StorageDepositArgs,
    pub prover_args: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct ClaimFeeArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
    pub native_fee_recipient: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug)]
pub struct BindTokenArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}
