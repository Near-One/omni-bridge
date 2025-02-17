use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_sdk::{assert_one_yocto, borsh, near};
use near_sdk::{env, near_bindgen, AccountId, NearToken};
use omni_types::{FastTransferStatus, TransferId};

use crate::{
    require, ChainKind, Contract, ContractExt, Fee, OmniAddress, Promise, SdkExpect,
    TransferMessage, U128,
};

pub const BRIDGE_TOKEN_INIT_BALANCE: NearToken = NearToken::from_near(3);
pub const NEP141_DEPOSIT: NearToken = NearToken::from_yoctonear(1_250_000_000_000_000_000_000);

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessageStorageValue {
    pub message: TransferMessage,
    pub owner: AccountId,
}

#[allow(clippy::module_name_repetitions)]
#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub enum TransferMessageStorage {
    V0(TransferMessageStorageValue),
}

impl TransferMessageStorage {
    pub fn into_main(self) -> TransferMessageStorageValue {
        match self {
            TransferMessageStorage::V0(m) => m,
        }
    }

    pub fn encode_borsh(
        message: TransferMessage,
        owner: AccountId,
    ) -> Result<Vec<u8>, std::io::Error> {
        borsh::to_vec(&TransferMessageStorage::V0(TransferMessageStorageValue {
            message,
            owner,
        }))
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimals {
    pub decimals: u8,
    pub origin_decimals: u8,
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

    pub fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let Some(storage) = self.storage_balance_of(&account_id) else {
            return false;
        };

        if !force.unwrap_or_default() {
            require!(
                storage.total.saturating_sub(storage.available)
                    == self.required_balance_for_account(),
                "This account owns some pending transfers, use `force=true` to ignore them."
            );
        }

        self.accounts_balances.remove(&account_id);

        let refund = self
            .required_balance_for_account()
            .saturating_add(storage.available);
        Promise::new(account_id).transfer(refund);
        true
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: self.required_balance_for_account(),
            max: None,
        }
    }

    pub fn storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        self.accounts_balances.get(account_id)
    }

    pub fn required_balance_for_account(&self) -> NearToken {
        let key_len = Self::max_key_len_of_account_id();
        let value_len: u64 = borsh::to_vec(&StorageBalance {
            total: NearToken::from_yoctonear(0),
            available: NearToken::from_yoctonear(0),
        })
        .sdk_expect("ERR_BORSH")
        .len()
        .try_into()
        .sdk_expect("ERR_CAST");

        env::storage_byte_cost()
            .saturating_mul((Self::get_basic_storage() + key_len + value_len).into())
    }

    pub fn required_balance_for_init_transfer(&self) -> NearToken {
        let max_account_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");

        let key_len: u64 = borsh::to_vec(&TransferId::default())
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST");

        let value_len: u64 =
            borsh::to_vec(&TransferMessageStorage::V0(TransferMessageStorageValue {
                message: TransferMessage {
                    origin_nonce: 0,
                    token: OmniAddress::Near(max_account_id.clone()),
                    amount: U128(0),
                    recipient: OmniAddress::Near(max_account_id.clone()),
                    fee: Fee::default(),
                    sender: OmniAddress::Near(max_account_id.clone()),
                    msg: String::new(),
                    destination_nonce: 0,
                    origin_transfer_id: Some(TransferId {
                        origin_chain: ChainKind::Near,
                        origin_nonce: 0,
                    }),
                },
                owner: max_account_id,
            }))
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST");

        env::storage_byte_cost()
            .saturating_mul((Self::get_basic_storage() + key_len + value_len).into())
    }

    pub fn required_balance_for_fin_transfer(&self) -> NearToken {
        let key_len: u64 = borsh::to_vec(&(ChainKind::Eth, 0_u64))
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST");

        let storage_cost =
            env::storage_byte_cost().saturating_mul((Self::get_basic_storage() + key_len).into());
        let ft_transfers_cost = NearToken::from_yoctonear(2);

        storage_cost.saturating_add(ft_transfers_cost)
    }

    pub fn required_balance_for_fast_transfer(&self) -> NearToken {
        let key_len = borsh::to_vec(&[0u8; 32]).sdk_expect("ERR_BORSH").len() as u64;

        let max_account_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");
        let value_len = borsh::to_vec(&FastTransferStatus {
            relayer: max_account_id.clone(),
            finalised: false,
        })
        .sdk_expect("ERR_BORSH")
        .len() as u64;

        let storage_cost = env::storage_byte_cost()
            .saturating_mul((Self::get_basic_storage() + key_len + value_len).into());
        let ft_transfers_cost = NearToken::from_yoctonear(1);

        storage_cost.saturating_add(ft_transfers_cost)
    }

    pub fn required_balance_for_bind_token(&self) -> NearToken {
        let max_token_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");

        let key_len: u64 = borsh::to_vec(&(ChainKind::Near, &max_token_id))
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST");

        let value_len: u64 = borsh::to_vec(&OmniAddress::Near(max_token_id))
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST");

        env::storage_byte_cost()
            .saturating_mul((3 * (Self::get_basic_storage() + key_len + value_len)).into())
    }

    pub fn required_balance_for_deploy_token(&self) -> NearToken {
        let key_len = Self::max_key_len_of_account_id();
        let deployed_tokens_required_balance =
            env::storage_byte_cost().saturating_mul((Self::get_basic_storage() + key_len).into());
        let bind_token_required_balance = self.required_balance_for_bind_token();

        bind_token_required_balance
            .saturating_add(deployed_tokens_required_balance)
            .saturating_add(BRIDGE_TOKEN_INIT_BALANCE)
            .saturating_add(NEP141_DEPOSIT)
    }

    fn get_basic_storage() -> u64 {
        const EXTRA_BYTES_RECORD: u64 = 40;
        const EXTRA_KEY_PREFIX_LEN: u64 = 1;
        EXTRA_BYTES_RECORD + EXTRA_KEY_PREFIX_LEN
    }

    fn max_key_len_of_account_id() -> u64 {
        let max_account_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");

        borsh::to_vec(&max_account_id)
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST")
    }
}
