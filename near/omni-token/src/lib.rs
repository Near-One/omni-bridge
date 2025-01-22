use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::{
    FungibleToken, FungibleTokenCore, FungibleTokenResolver,
};
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::serde_json::json;
use near_sdk::{
    env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseOrValue,
    PublicKey, StorageUsage,
};
use omni_ft::{MetadataManagment, MintAndBurn, UpgradeAndMigrate};
use omni_types::BasicMetadata;

const OUTER_UPGRADE_GAS: Gas = Gas::from_tgas(15);
const NO_DEPOSIT: NearToken = NearToken::from_yoctonear(0);
const CURRENT_STATE_VERSION: u32 = 2;

pub mod omni_ft;

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct OmniToken {
    controller: AccountId,
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
}

#[near]
impl OmniToken {
    #[init]
    pub fn new(controller: AccountId, metadata: BasicMetadata) -> Self {
        let current_account_id = env::current_account_id();
        let deployer_account = current_account_id
            .get_parent_account_id()
            .unwrap_or_else(|| env::panic_str("ERR_INVALID_PARENT_ACCOUNT"));

        require!(
            env::predecessor_account_id().as_str() == deployer_account,
            "Only the deployer account can init this contract"
        );

        Self {
            controller,
            token: FungibleToken::new(b"t".to_vec()),
            metadata: LazyOption::new(
                b"m".to_vec(),
                Some(&FungibleTokenMetadata {
                    spec: FT_METADATA_SPEC.to_string(),
                    name: metadata.name,
                    symbol: metadata.symbol,
                    icon: None,
                    reference: None,
                    reference_hash: None,
                    decimals: metadata.decimals,
                }),
            ),
        }
    }

    /// Attach a new full access to the current contract.
    pub fn attach_full_access_key(&mut self, public_key: PublicKey) -> Promise {
        self.assert_controller();
        Promise::new(env::current_account_id()).add_full_access_key(public_key)
    }

    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    fn assert_controller(&self) {
        let caller = env::predecessor_account_id();
        require!(caller == self.controller, "ERR_MISSING_PERMISSION");
    }

    //Withdraw to Ethereum. Deprecated function for backward compatibility
    #[payable]
    pub fn withdraw(&mut self, amount: U128, recipient: String) -> Promise {
        let pov: PromiseOrValue<U128> = self.token.ft_transfer_call(
            self.controller.clone(),
            amount,
            None,
            "eth:".to_string() + &recipient,
        );

        match pov {
            near_sdk::PromiseOrValue::Promise(promise) => promise,
            near_sdk::PromiseOrValue::Value(_) => env::panic_str("Expected Promise")
        }
    }

    pub fn account_storage_usage(&self) -> StorageUsage {
        self.token.account_storage_usage
    }
}

#[near]
impl MintAndBurn for OmniToken {
    #[payable]
    fn mint(
        &mut self,
        account_id: AccountId,
        amount: U128,
        msg: Option<String>,
    ) -> PromiseOrValue<U128> {
        self.assert_controller();

        if let Some(msg) = msg {
            self.token
                .internal_deposit(&env::predecessor_account_id(), amount.into());

            self.ft_transfer_call(account_id, amount, None, msg)
        } else {
            self.token.internal_deposit(&account_id, amount.into());
            PromiseOrValue::Value(amount)
        }
    }

    fn burn(&mut self, amount: U128) {
        self.assert_controller();

        self.token
            .internal_withdraw(&env::predecessor_account_id(), amount.into());
    }
}

#[near]
impl MetadataManagment for OmniToken {
    fn set_metadata(
        &mut self,
        name: Option<String>,
        symbol: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
        decimals: Option<u8>,
        icon: Option<String>,
    ) {
        self.assert_controller();

        let mut metadata = self.ft_metadata();
        if let Some(name) = name {
            metadata.name = name;
        }
        if let Some(symbol) = symbol {
            metadata.symbol = symbol;
        }
        if let Some(reference) = reference {
            metadata.reference = Some(reference);
        }
        if let Some(reference_hash) = reference_hash {
            metadata.reference_hash = Some(reference_hash);
        }
        if let Some(decimals) = decimals {
            // Decimals can't be changed if it's already set.
            if decimals != 0 {
                metadata.decimals = decimals;
            }
        }
        if let Some(icon) = icon {
            metadata.icon = Some(icon);
        }

        self.metadata.set(&metadata);
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

#[near]
impl FungibleTokenCore for OmniToken {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo);
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for OmniToken {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let (used_amount, _burned_amount) =
            self.token
                .internal_ft_resolve_transfer(&sender_id, receiver_id, amount);

        used_amount.into()
    }
}

#[near]
impl StorageManagement for OmniToken {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        self.token.storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        self.token.internal_storage_unregister(force).is_some()
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenMetadataProvider for OmniToken {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata
            .get()
            .unwrap_or_else(|| FungibleTokenMetadata {
                spec: FT_METADATA_SPEC.to_string(),
                name: String::default(),
                symbol: String::default(),
                icon: None,
                reference: None,
                reference_hash: None,
                decimals: 0,
            })
    }
}

pub type Mask = u128;

#[derive(BorshDeserialize, BorshSerialize)]
pub struct BridgeTokenV1 {
    controller: AccountId,
    token: FungibleToken,
    name: String,
    symbol: String,
    reference: String,
    reference_hash: Base64VecU8,
    decimals: u8,
    paused: Mask,
    icon: Option<String>,
}

impl From<BridgeTokenV1> for OmniToken {
    fn from(obj: BridgeTokenV1) -> Self {
        #[allow(deprecated)]
        Self {
            controller: obj.controller,
            token: obj.token,
            metadata: LazyOption::new(
                b"m".to_vec(),
                Some(&FungibleTokenMetadata {
                    spec: FT_METADATA_SPEC.to_string(),
                    name: obj.name,
                    symbol: obj.symbol,
                    icon: obj.icon,
                    reference: Some(obj.reference),
                    reference_hash: Some(obj.reference_hash.into()),
                    decimals: obj.decimals,
                }),
            ),
        }
    }
}

#[near]
impl OmniToken {
    /// This function can only be called from the factory or from the contract itself.
    #[init(ignore_state)]
    pub fn migrate(from_version: u32) -> Self {
        if from_version == 1 {
            let old_state: BridgeTokenV1 = env::state_read().expect("Contract isn't initialized");
            let new_state: OmniToken = old_state.into();
            new_state.assert_controller();
            new_state
        } else {
            env::state_read().unwrap()
        }
    }
}
