use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_sdk::{assert_one_yocto, borsh, near};
use near_sdk::{env, near_bindgen, AccountId, NearToken};
use omni_types::{FastTransferStatus, Nonce, TransferId, TransferIdKind, UnifiedTransferId};

use crate::{
    require, ChainKind, Contract, ContractExt, Fee, OmniAddress, Promise, SdkExpect,
    TransferMessage, U128,
};

pub const BRIDGE_TOKEN_INIT_BALANCE: NearToken = NearToken::from_near(3);
pub const NEP141_DEPOSIT: NearToken = NearToken::from_yoctonear(1_250_000_000_000_000_000_000);

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessageV0 {
    pub origin_nonce: Nonce,
    pub token: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
    pub msg: String,
    pub destination_nonce: Nonce,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessageV1 {
    pub origin_nonce: Nonce,
    pub token: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
    pub msg: String,
    pub destination_nonce: Nonce,
    pub origin_transfer_id: Option<TransferId>,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessageStorageValueV0 {
    pub message: TransferMessageV0,
    pub owner: AccountId,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessageStorageValueV1 {
    pub message: TransferMessageV1,
    pub owner: AccountId,
}

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
    V0(TransferMessageStorageValueV0),
    V1(TransferMessageStorageValueV1),
    V2(TransferMessageStorageValue),
}

impl TransferMessageStorage {
    pub fn into_main(self) -> TransferMessageStorageValue {
        match self {
            Self::V0(m) => TransferMessageStorageValue {
                message: TransferMessage {
                    origin_nonce: m.message.origin_nonce,
                    token: m.message.token,
                    amount: m.message.amount,
                    recipient: m.message.recipient,
                    fee: m.message.fee,
                    sender: m.message.sender,
                    msg: m.message.msg,
                    destination_nonce: m.message.destination_nonce,
                    origin_transfer_id: None,
                },
                owner: m.owner,
            },
            Self::V1(m) => TransferMessageStorageValue {
                message: TransferMessage {
                    origin_nonce: m.message.origin_nonce,
                    token: m.message.token,
                    amount: m.message.amount,
                    recipient: m.message.recipient,
                    fee: m.message.fee,
                    sender: m.message.sender,
                    msg: m.message.msg,
                    destination_nonce: m.message.destination_nonce,
                    origin_transfer_id: m.message.origin_transfer_id.map(|m| UnifiedTransferId {
                        origin_chain: m.origin_chain,
                        kind: TransferIdKind::Nonce(m.origin_nonce),
                    }),
                },
                owner: m.owner,
            },
            Self::V2(m) => m,
        }
    }

    pub fn encode_borsh(
        message: TransferMessage,
        owner: AccountId,
    ) -> Result<Vec<u8>, std::io::Error> {
        borsh::to_vec(&Self::V2(TransferMessageStorageValue { message, owner }))
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub enum FastTransferStatusStorage {
    V0(FastTransferStatus),
}

impl FastTransferStatusStorage {
    pub fn into_main(self) -> FastTransferStatus {
        match self {
            Self::V0(status) => status,
        }
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
        let storage = self.accounts_balances.get(&account_id).map_or_else(
            || {
                let min_required_storage_balance = self.required_balance_for_account();
                let available = amount
                    .checked_sub(min_required_storage_balance)
                    .sdk_expect("The attached deposit is less than the minimum storage balance");
                StorageBalance {
                    total: amount,
                    available,
                }
            },
            |mut storage| {
                storage.total = storage.total.saturating_add(amount);
                storage.available = storage.available.saturating_add(amount);
                storage
            },
        );
        self.accounts_balances.insert(&account_id, &storage);

        if let Some(promise_id) = &self.init_transfer_promises.get(&account_id) {
            let result = env::promise_yield_resume(promise_id, []);
            env::log_str(&format!("Init transfer resume. Result: {result}"));
        }

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

        Promise::new(account_id).transfer(to_withdraw).detach();

        storage
    }

    #[payable]
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
        Promise::new(account_id).transfer(refund).detach();
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

    pub(crate) fn has_storage_balance(&self, account_id: &AccountId, balance: NearToken) -> bool {
        match self.storage_balance_of(account_id) {
            Some(storage_balance) => storage_balance.available >= balance,
            None => false,
        }
    }

    // Used when native fee for the transfer is deposited to the dedicated message account.
    // Deducts the total balance from `account_id` and credits it to `storage_payer`.
    // Returns an error if `account_id` does not have enough balance to cover `native_fee` or if `storage_payer` doesn't have enough balance to complete the transfer.
    pub(crate) fn try_to_transfer_balance_from_message_account(
        &mut self,
        account_id: &AccountId,
        native_fee: NearToken,
        storage_payer: &AccountId,
        required_storage_payer_balance: NearToken,
    ) -> Result<(), String> {
        let balance = self
            .accounts_balances
            .get(account_id)
            .ok_or("ERR_MESSAGE_ACCOUNT_NOT_REGISTERED")?;

        if balance.total < native_fee {
            return Err("ERR_NOT_ENOUGH_BALANCE_FOR_FEE".to_string());
        }

        let mut storage = self
            .accounts_balances
            .get(storage_payer)
            .ok_or("ERR_SIGNER_NOT_REGISTERED")?;

        storage.available = storage.available.saturating_add(balance.total);

        if storage.available < required_storage_payer_balance.saturating_add(native_fee) {
            return Err("ERR_SIGNER_NOT_ENOUGH_BALANCE".to_string());
        }

        self.accounts_balances.insert(storage_payer, &storage);
        self.accounts_balances.remove(account_id);
        Ok(())
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

    pub fn required_balance_for_init_transfer(&self, msg: Option<String>) -> NearToken {
        let max_account_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");

        self.required_balance_for_init_transfer_message(TransferMessage {
            origin_nonce: 0,
            token: OmniAddress::Near(max_account_id.clone()),
            amount: U128(0),
            recipient: OmniAddress::Near(max_account_id.clone()),
            fee: Fee::default(),
            sender: OmniAddress::Near(max_account_id.clone()),
            msg: msg.unwrap_or_default(),
            destination_nonce: 0,
            origin_transfer_id: Some(UnifiedTransferId {
                origin_chain: ChainKind::Eth,
                kind: TransferIdKind::Utxo({
                    omni_types::UtxoId {
                        tx_hash: "a".repeat(64),
                        vout: 0,
                    }
                }),
            }),
        })
    }

    pub fn required_balance_for_init_transfer_message(
        &self,
        transfer_message: TransferMessage,
    ) -> NearToken {
        let max_account_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");

        let key_len: u64 = borsh::to_vec(&transfer_message.get_transfer_id())
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST");

        let value_len: u64 =
            borsh::to_vec(&TransferMessageStorage::V2(TransferMessageStorageValue {
                message: transfer_message,
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
        let key_len: u64 = borsh::to_vec(&[0u8; 32])
            .sdk_expect("ERR_BORSH")
            .len()
            .try_into()
            .sdk_expect("ERR_CAST");

        let max_account_id: AccountId = "a".repeat(64).parse().sdk_expect("ERR_PARSE_ACCOUNT_ID");
        let value_len: u64 = borsh::to_vec(&FastTransferStatusStorage::V0(FastTransferStatus {
            relayer: max_account_id.clone(),
            finalised: false,
            storage_owner: max_account_id,
        }))
        .sdk_expect("ERR_BORSH")
        .len()
        .try_into()
        .sdk_expect("ERR_CAST");

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

    const fn get_basic_storage() -> u64 {
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
