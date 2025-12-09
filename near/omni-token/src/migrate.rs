use borsh::{BorshDeserialize, BorshSerialize};
use near_contract_standards::fungible_token::{FungibleToken, metadata::FungibleTokenMetadata};
use near_sdk::{AccountId, Gas, NearToken, collections::LazyOption, env, near, require, store::Lazy};
use crate::{OmniToken, OmniTokenExt, WITHDRAW_RELAYER_ADDRESS, omni_ft::UpgradeAndMigrate};
use near_sdk::serde_json::json;

const CURRENT_STATE_VERSION: u32 = 3;
const OUTER_UPGRADE_GAS: Gas = Gas::from_tgas(15);
const NO_DEPOSIT: NearToken = NearToken::from_yoctonear(0);
const STATE_KEY: &[u8] = b"STATE";
const OWNABLE_KEY: &[u8] = b"OWNABLE";

#[derive(BorshDeserialize, BorshSerialize)]
pub struct NearIntentsState {
    pub token: FungibleToken,
    pub metadata: Lazy<FungibleTokenMetadata>,
}

#[near]
impl OmniToken {
    #[private]
    #[init(ignore_state)]
    #[allow(unused_variables)]
    pub fn migrate(controller: AccountId, withdraw_relayer: Option<AccountId>) -> Self {
        if !env::state_exists() {
            env::panic_str("Old state not found. Migration is not needed.")
        }

        let state = env::storage_read(STATE_KEY)
            .unwrap_or_else(|| env::panic_str("Failed to read state key."));

        if let Ok(state) = NearIntentsState::try_from_slice(&state) {
            require!(env::storage_remove(OWNABLE_KEY), "Wrong token version for migration");

            if let Some(relayer) = withdraw_relayer {
                env::storage_write(WITHDRAW_RELAYER_ADDRESS, &borsh::to_vec(&relayer).unwrap());
            }
            
            Self {
                controller,
                token: state.token,
                metadata: LazyOption::new(
                    b"m".to_vec(),
                    Some(&state.metadata.get()),
                ),
            }
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
        let code = env::input().unwrap_or_else(|| env::panic_str("ERR_NO_INPUT"));
        // Deploy the contract code.
        let promise_id = env::promise_batch_create(&env::current_account_id());
        env::promise_batch_action_deploy_contract(promise_id, &code);
        // Call promise to migrate the state.
        // Batched together to fail upgrade if migration fails.
        env::promise_batch_action_function_call(
            promise_id,
            "migrate",
            &json!({ "from_version": CURRENT_STATE_VERSION })
                .to_string()
                .into_bytes(),
            NO_DEPOSIT,
            env::prepaid_gas()
                .saturating_sub(env::used_gas())
                .saturating_sub(OUTER_UPGRADE_GAS),
        );
        env::promise_return(promise_id);
    }
}