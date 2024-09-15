use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_sdk::{assert_one_yocto, borsh};
use near_sdk::{env, near_bindgen, AccountId, NearToken};

use crate::*;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub enum TransferMessageStorage {
    V0((TransferMessage, AccountId)),
}

impl TransferMessageStorage {
    pub fn into_main(self) -> (TransferMessage, AccountId) {
        match self {
            TransferMessageStorage::V0(m) => m,
        }
    }

    pub fn encode_borsh(
        message: &TransferMessage,
        account: &AccountId,
    ) -> Result<Vec<u8>, std::io::Error> {
        #[derive(BorshSerialize)]
        enum RefTransferMessageStorage<'a> {
            V0((&'a TransferMessage, &'a AccountId)),
        }

        borsh::to_vec(&RefTransferMessageStorage::V0((message, account)))
    }
}

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<AccountId>) -> StorageBalance {
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        let amount = env::attached_deposit();
        let storage = if let Some(mut storage) = self.accounts_balances.get(&account_id) {
            storage.total = storage.total.saturating_add(amount);
            storage.available = storage.available.saturating_add(amount);
            storage
        } else {
            let min_required_storage_balance = self.required_balance_for_account();
            let available = amount
                .checked_sub(min_required_storage_balance)
                .sdk_expect("The attached deposit is less than the minimum storage balance");
            StorageBalance {
                total: amount,
                available,
            }
        };

        self.accounts_balances.insert(&account_id, &storage);
        storage
    }

    #[payable]
    pub fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        assert_one_yocto();
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

        self.accounts_balances.insert(&account_id, &storage);
        storage
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: self.required_balance_for_account(),
            max: None,
        }
    }

    pub fn storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        self.accounts_balances.get(&account_id)
    }

    pub fn required_balance_for_account(&self) -> NearToken {
        let key_len = 68;
        let value_len = borsh::to_vec(&StorageBalance {
            total: NearToken::from_yoctonear(0),
            available: NearToken::from_yoctonear(0),
        })
        .sdk_expect("ERR_BORSH")
        .len() as u64;

        env::storage_byte_cost()
            .saturating_mul((Self::get_basic_storage() + key_len + value_len).into())
    }

    pub fn required_balance_for_init_transfer(
        &self,
        recipient: OmniAddress,
        sender: OmniAddress,
    ) -> NearToken {
        let key_len = borsh::to_vec(&0_u128).sdk_expect("ERR_BORSH").len() as u64;
        let max_account_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");
        let value_len = borsh::to_vec(&TransferMessageStorage::V0((
            TransferMessage {
                origin_nonce: U128(0),
                token: max_account_id.clone(),
                amount: U128(0),
                recipient,
                fee: U128(0),
                sender,
            },
            max_account_id,
        )))
        .sdk_expect("ERR_BORSH")
        .len() as u64;

        env::storage_byte_cost()
            .saturating_mul((Self::get_basic_storage() + key_len + value_len).into())
    }

    pub fn required_balance_for_fin_transfer(&self) -> NearToken {
        let key_len = borsh::to_vec(&(ChainKind::Eth, 0_u128))
            .sdk_expect("ERR_BORSH")
            .len() as u64;

        env::storage_byte_cost().saturating_mul((Self::get_basic_storage() + key_len).into())
    }

    fn get_basic_storage() -> u64 {
        const EXTRA_BYTES_RECORD: u64 = 40;
        const EXTRA_KEY_PREFIX_LEN: u64 = 1;
        EXTRA_BYTES_RECORD + EXTRA_KEY_PREFIX_LEN
    }
}
