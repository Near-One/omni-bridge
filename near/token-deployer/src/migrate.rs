use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::{collections::UnorderedSet, env, near, AccountId, PanicOnDefault, PublicKey};

use crate::{TokenDeployer, TokenDeployerExt};

const STATE_KEY: &[u8] = b"STATE";

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct OldState {}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldLegacyState {
    pub prover_account: AccountId,
    pub locker_address: [u8; 20],
    pub tokens: UnorderedSet<String>,
    pub used_events: UnorderedSet<Vec<u8>>,
    pub owner_pk: PublicKey,
    pub bridge_token_storage_deposit_required: u128,
    paused: u128,
}

#[near]
impl TokenDeployer {
    #[private]
    #[init(ignore_state)]
    pub fn migrate(omni_token_global_contract_id: AccountId) -> Self {
        if !env::state_exists() {
            env::panic_str("Old state not found. Migration is not needed.")
        }

        let state = env::storage_read(STATE_KEY)
            .unwrap_or_else(|| env::panic_str("Failed to read state key."));

        if OldState::try_from_slice(&state).is_ok()
            || OldLegacyState::try_from_slice(&state).is_ok()
        {
            Self {
                omni_token_global_contract_id,
            }
        } else {
            env::panic_str("Old state not found. Migration is not needed.")
        }
    }
}
