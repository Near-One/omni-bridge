use crate::{
    storage::{Decimals, FastTransferStatusStorage, TransferMessageStorage},
    Contract, ContractExt, StorageKey,
};
use borsh::{BorshDeserialize, BorshSerialize};
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::{
    collections::{LookupMap, LookupSet},
    env, near, AccountId, PanicOnDefault,
};
use omni_types::{ChainKind, FastTransferId, Nonce, OmniAddress, TransferId};

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct StateV0 {
    pub prover_account: AccountId,
    pub factories: LookupMap<ChainKind, OmniAddress>,
    pub pending_transfers: LookupMap<TransferId, TransferMessageStorage>,
    pub finalised_transfers: LookupSet<TransferId>,
    pub token_id_to_address: LookupMap<(ChainKind, AccountId), OmniAddress>,
    pub token_address_to_id: LookupMap<OmniAddress, AccountId>,
    pub token_decimals: LookupMap<OmniAddress, Decimals>,
    pub deployed_tokens: LookupSet<AccountId>,
    pub token_deployer_accounts: LookupMap<ChainKind, AccountId>,
    pub mpc_signer: AccountId,
    pub current_origin_nonce: Nonce,
    pub destination_nonces: LookupMap<ChainKind, Nonce>,
    pub accounts_balances: LookupMap<AccountId, StorageBalance>,
    pub wnear_account_id: AccountId,
}

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct StateV1 {
    pub prover_account: AccountId,
    pub factories: LookupMap<ChainKind, OmniAddress>,
    pub pending_transfers: LookupMap<TransferId, TransferMessageStorage>,
    pub finalised_transfers: LookupSet<TransferId>,
    pub fast_transfers: LookupMap<FastTransferId, FastTransferStatusStorage>,
    pub token_id_to_address: LookupMap<(ChainKind, AccountId), OmniAddress>,
    pub token_address_to_id: LookupMap<OmniAddress, AccountId>,
    pub token_decimals: LookupMap<OmniAddress, Decimals>,
    pub deployed_tokens: LookupSet<AccountId>,
    pub token_deployer_accounts: LookupMap<ChainKind, AccountId>,
    pub mpc_signer: AccountId,
    pub current_origin_nonce: Nonce,
    pub destination_nonces: LookupMap<ChainKind, Nonce>,
    pub accounts_balances: LookupMap<AccountId, StorageBalance>,
    pub wnear_account_id: AccountId,
}

#[near]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate(btc_connector: AccountId) -> Self {
        if let Some(old_state) = env::state_read::<StateV0>() {
            Self {
                prover_account: old_state.prover_account,
                factories: old_state.factories,
                pending_transfers: old_state.pending_transfers,
                finalised_transfers: old_state.finalised_transfers,
                fast_transfers: LookupMap::new(StorageKey::FastTransfers),
                token_id_to_address: old_state.token_id_to_address,
                token_address_to_id: old_state.token_address_to_id,
                token_decimals: old_state.token_decimals,
                deployed_tokens: old_state.deployed_tokens,
                token_deployer_accounts: old_state.token_deployer_accounts,
                mpc_signer: old_state.mpc_signer,
                current_origin_nonce: old_state.current_origin_nonce,
                destination_nonces: old_state.destination_nonces,
                accounts_balances: old_state.accounts_balances,
                wnear_account_id: old_state.wnear_account_id,
                btc_connector,
            }
        } else if let Some(old_state) = env::state_read::<StateV1>() {
            Self {
                prover_account: old_state.prover_account,
                factories: old_state.factories,
                pending_transfers: old_state.pending_transfers,
                finalised_transfers: old_state.finalised_transfers,
                fast_transfers: old_state.fast_transfers,
                token_id_to_address: old_state.token_id_to_address,
                token_address_to_id: old_state.token_address_to_id,
                token_decimals: old_state.token_decimals,
                deployed_tokens: old_state.deployed_tokens,
                token_deployer_accounts: old_state.token_deployer_accounts,
                mpc_signer: old_state.mpc_signer,
                current_origin_nonce: old_state.current_origin_nonce,
                destination_nonces: old_state.destination_nonces,
                accounts_balances: old_state.accounts_balances,
                wnear_account_id: old_state.wnear_account_id,
                btc_connector,
            }
        } else {
            env::panic_str("Old state not found. Migration is not needed.")
        }
    }
}
