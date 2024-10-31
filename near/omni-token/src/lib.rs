use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::{
    Balance, FungibleToken, FungibleTokenCore, FungibleTokenResolver,
};
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::serde_json::json;
use near_sdk::{
    assert_one_yocto, env, ext_contract, near, require, AccountId, Gas, NearToken, PanicOnDefault,
    Promise, PromiseOrValue, PublicKey, StorageUsage,
};
/// Gas to call finish withdraw method on factory.
const FINISH_WITHDRAW_GAS: Gas = Gas::from_tgas(50);
const OUTER_UPGRADE_GAS: Gas = Gas::from_tgas(15);
const NO_DEPOSIT: NearToken = NearToken::from_yoctonear(0);
const CURRENT_STATE_VERSION: u32 = 1;

pub type Mask = u128;

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct OmniToken {
    controller: AccountId,
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    paused: Mask,
}

#[ext_contract(ext_omni_factory)]
pub trait ExtOmniTokenFactory {
    #[result_serializer(borsh)]
    fn finish_withdraw(
        &self,
        #[serializer(borsh)] amount: Balance,
        #[serializer(borsh)] recipient: String,
    ) -> Promise;
}

#[near]
impl OmniToken {
    #[init]
    pub fn new(controller: AccountId, metadta: Option<FungibleTokenMetadata>) -> Self {
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
            metadata: LazyOption::new(b"m".to_vec(), metadta.as_ref()),
            paused: Mask::default(),
        }
    }

    pub fn set_metadata(
        &mut self,
        name: Option<String>,
        symbol: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
        decimals: Option<u8>,
        icon: Option<String>,
    ) {
        require!(self.controller_or_self());

        let mut metadata = self.ft_metadata();
        name.map(|name| metadata.name = name);
        symbol.map(|symbol| metadata.symbol = symbol);
        reference.map(|reference| metadata.reference = Some(reference));
        reference_hash.map(|reference_hash| metadata.reference_hash = Some(reference_hash));
        decimals.map(|decimals| metadata.decimals = decimals);
        icon.map(|icon| metadata.icon = Some(icon));

        self.metadata.set(&metadata);
    }

    #[payable]
    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        assert_eq!(
            env::predecessor_account_id(),
            self.controller,
            "Only controller can call mint"
        );

        self.storage_deposit(Some(account_id.clone()), None);
        self.token.internal_deposit(&account_id, amount.into());
    }

    #[payable]
    pub fn withdraw(&mut self, amount: U128, recipient: String) -> Promise {
        require!(!self.is_paused());
        assert_one_yocto();

        self.token
            .internal_withdraw(&env::predecessor_account_id(), amount.into());

        ext_omni_factory::ext(self.controller.clone())
            .with_static_gas(FINISH_WITHDRAW_GAS)
            .finish_withdraw(amount.into(), recipient)
    }

    pub fn account_storage_usage(&self) -> StorageUsage {
        self.token.account_storage_usage
    }

    /// Return true if the caller is either controller or self
    pub fn controller_or_self(&self) -> bool {
        let caller = env::predecessor_account_id();
        caller == self.controller || caller == env::current_account_id()
    }

    pub fn is_paused(&self) -> bool {
        self.paused != 0 && !self.controller_or_self()
    }

    pub fn set_paused(&mut self, paused: bool) {
        require!(self.controller_or_self());
        self.paused = if paused { 1 } else { 0 };
    }

    pub fn upgrade_and_migrate(&self) {
        require!(
            self.controller_or_self(),
            "Only the controller or self can update the code"
        );

        // Receive the code directly from the input to avoid the
        // GAS overhead of deserializing parameters
        let code = env::input().unwrap_or_else(|| panic!("ERR_NO_INPUT"));
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

    #[private]
    #[init(ignore_state)]
    pub fn migrate(_from_version: u32) -> Self {
        env::state_read().unwrap_or_else(|| env::panic_str("ERR_FAILED_TO_READ_STATE"))
    }

    /// Attach a new full access to the current contract.
    pub fn attach_full_access_key(&mut self, public_key: PublicKey) -> Promise {
        require!(self.controller_or_self());
        Promise::new(env::current_account_id()).add_full_access_key(public_key)
    }

    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }
}

#[near]
impl FungibleTokenCore for OmniToken {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo)
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
