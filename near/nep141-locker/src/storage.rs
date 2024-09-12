use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_sdk::{env, near_bindgen, AccountId, NearToken};

use crate::*;

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<AccountId>) -> StorageBalance {
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        let amount = env::attached_deposit();
        let storage = if let Some(mut storage) = self.accounts.get(&account_id) {
            storage.total = storage.total.saturating_add(amount);
            storage.available = storage.available.saturating_add(amount);
            storage
        } else {
            let min_required_storage_balance =
                env::storage_byte_cost().saturating_mul(self.account_storage_usage.into());
            let total = amount
                .checked_sub(min_required_storage_balance)
                .sdk_expect("The attached deposit is less than the minimum storage balance");
            StorageBalance {
                total,
                available: total,
            }
        };

        self.accounts.insert(&account_id, &storage);
        storage
    }

    pub fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        let account_id = env::predecessor_account_id();
        let mut storage = self
            .storage_balance_of(&account_id)
            .sdk_expect("The account is not registered");
        let to_withdraw = amount.unwrap_or(storage.available);
        storage.total = storage
            .total
            .checked_sub(to_withdraw)
            .sdk_expect("The amount is greater than the total storage balance");
        storage.available = storage
            .available
            .checked_sub(to_withdraw)
            .sdk_expect("The amount is greater than the available storage balance");

        if storage.total == NearToken::from_yoctonear(0) {
            self.accounts.remove(&account_id);
        } else {
            self.accounts.insert(&account_id, &storage);
        }

        storage
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        let required_storage_balance =
            env::storage_byte_cost().saturating_mul(self.account_storage_usage.into());
        StorageBalanceBounds {
            min: required_storage_balance,
            max: None,
        }
    }

    pub fn storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        self.accounts.get(&account_id)
    }
}
