use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::{env, near, AccountId, PanicOnDefault};

use crate::{TokenDeployer, TokenDeployerExt};

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct OldState {}

#[near]
impl TokenDeployer {
    #[private]
    #[init(ignore_state)]
    pub fn migrate(omni_token_global_contract_id: AccountId) -> Self {
        if env::state_read::<OldState>().is_some() {
            Self {
                omni_token_global_contract_id,
            }
        } else {
            env::panic_str("Old state not found. Migration is not needed.")
        }
    }
}
