use crate::{omni_ft::UpgradeAndMigrate, OmniToken, OmniTokenExt, WITHDRAW_RELAYER_ADDRESS};
use borsh::{BorshDeserialize, BorshSerialize};
use near_contract_standards::fungible_token::{metadata::FungibleTokenMetadata, FungibleToken};
use near_sdk::serde_json::json;
use near_sdk::{
    collections::LazyOption, env, near, require, store::Lazy, AccountId, Gas, GasWeight, NearToken,
};
use omni_types::TokenError;

const CURRENT_STATE_VERSION: u32 = 3;
const NO_DEPOSIT: NearToken = NearToken::from_yoctonear(0);
const STATE_KEY: &[u8] = b"STATE";
const OWNABLE_KEY: &[u8] = b"__OWNER__";

#[derive(BorshDeserialize, BorshSerialize)]
pub struct NearIntentsState {
    pub token: FungibleToken,
    pub metadata: Lazy<FungibleTokenMetadata>,
}

#[near]
impl OmniToken {
    /// # Panics
    ///
    /// This function will panic if token is not in the expected state.
    #[private]
    #[init(ignore_state)]
    #[allow(unused_variables)]
    pub fn migrate(from_version: u32) -> Self {
        env::state_read().unwrap_or_else(|| env::panic_str(TokenError::NoStateToMigrate.as_ref()))
    }

    /// # Panics
    ///
    /// This function will panic if token is not in the expected state.
    #[private]
    #[init(ignore_state)]
    pub fn migrate_from_poa(controller: AccountId, withdraw_relayer: &AccountId) -> Self {
        if !env::state_exists() {
            env::panic_str("Old state not found. Migration is not needed.")
        }

        let state = env::storage_read(STATE_KEY)
            .unwrap_or_else(|| env::panic_str("Failed to read state key."));

        if let Ok(state) = NearIntentsState::try_from_slice(&state) {
            require!(
                env::storage_remove(OWNABLE_KEY),
                "Wrong token version for migration: __OWNER__ key not found"
            );

            env::storage_write(
                WITHDRAW_RELAYER_ADDRESS,
                &borsh::to_vec(withdraw_relayer).unwrap(),
            );

            let new_state = Self {
                controller,
                token: state.token,
                metadata: LazyOption::new(b"m".to_vec(), Some(state.metadata.get())),
            };

            let mut old_metadata = state.metadata;
            old_metadata.remove();

            new_state
        } else {
            env::panic_str("Old state not found. Migration is not needed.")
        }
    }
}

#[near]
impl UpgradeAndMigrate for OmniToken {
    fn upgrade_and_migrate(&self) {
        self.assert_controller();

        // Receive the code directly from the input to avoid the
        // GAS overhead of deserializing parameters
        let input = env::input().unwrap_or_else(|| env::panic_str(TokenError::NoInput.as_ref()));
        let promise_id = env::promise_batch_create(&env::current_account_id());
        // Allow switching to global contract code when a hash is provided and vice versa.
        if input.len() == 32 {
            let code_hash = input
                .as_slice()
                .try_into()
                .unwrap_or_else(|_| env::panic_str(TokenError::InvalidCodeHash.as_ref()));
            env::promise_batch_action_use_global_contract(promise_id, &code_hash);
        } else {
            // Deploy the contract code.
            env::promise_batch_action_deploy_contract(promise_id, &input);
        }
        // Call promise to migrate the state.
        // Batched together to fail upgrade if migration fails.
        env::promise_batch_action_function_call_weight(
            promise_id,
            "migrate",
            &json!({ "from_version": CURRENT_STATE_VERSION })
                .to_string()
                .into_bytes(),
            NO_DEPOSIT,
            Gas::default(),
            GasWeight::default(),
        );
        env::promise_return(promise_id);
    }
}
