use std::collections::HashMap;

use crate::{
    storage::{Decimals, FastTransferStatusStorage, TransferMessageStorage},
    Contract, ContractExt, StorageKey,
};
use borsh::{BorshDeserialize, BorshSerialize};
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::{
    collections::{LookupMap, LookupSet, UnorderedMap},
    env, near, AccountId, CryptoHash, PanicOnDefault,
};
use omni_types::{btc::UTXOChainConfig, ChainKind, FastTransferId, Nonce, OmniAddress, TransferId};

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct OldState {
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
    pub provers: UnorderedMap<ChainKind, AccountId>,
    pub init_transfer_promises: LookupMap<AccountId, CryptoHash>,
    pub utxo_chain_connectors: HashMap<ChainKind, UTXOChainConfig>,
}

#[near]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        if let Some(old_state) = env::state_read::<OldState>() {
            Self {
                factories: old_state.factories,
                pending_transfers: old_state.pending_transfers,
                finalised_transfers: old_state.finalised_transfers,
                finalised_utxo_transfers: LookupSet::new(StorageKey::FinalisedUtxoTransfers),
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
                provers: old_state.provers,
                init_transfer_promises: old_state.init_transfer_promises,
                utxo_chain_connectors: old_state.utxo_chain_connectors,
            }
        } else {
            env::panic_str("Old state not found. Migration is not needed.")
        }
    }
}
