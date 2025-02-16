use near_sdk::{near, AccountId};

use crate::{ChainKind, OmniAddress};

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct StorageDepositAction {
    pub token_id: AccountId,
    pub account_id: AccountId,
    pub storage_deposit_amount: Option<u128>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct FinTransferArgs {
    pub chain_kind: ChainKind,
    pub storage_deposit_actions: Vec<StorageDepositAction>,
    pub prover_args: Vec<u8>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct ClaimFeeArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct BindTokenArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct DeployTokenArgs {
    pub chain_kind: ChainKind,
    pub prover_args: Vec<u8>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct AddDeployedTokenArgs {
    pub token_id: AccountId,
    pub token_address: OmniAddress,
    pub decimals: u8,
}
