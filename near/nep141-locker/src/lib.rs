use near_plugins::{
    access_control, access_control_any, pause, AccessControlRole, AccessControllable, Pausable,
    Upgradable,
};

use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, PanicOnDefault, Promise,
    PromiseOrValue,
};

mod types;
use types::*;

const LOG_METADATA_GAS: Gas = Gas::from_tgas(10);
const LOG_METADATA_CALLBCAK_GAS: Gas = Gas::from_tgas(30);
const MPC_SIGNING_GAS: Gas = Gas::from_tgas(200);
const SIGN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(5);

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    PendingTransfers,
    Factories,
}

#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    DAO,
    PauseManager,
    UnrestrictedDeposit,
    UpgradableCodeStager,
    UpgradableCodeDeployer,
}

#[ext_contract(ext_self)]
pub trait ExtContract {
    fn log_metadata_callbcak(
        &self,
        #[callback] metadata: FungibleTokenMetadata,
        token_id: AccountId,
    );
    fn sign_transfer_callback(
        &self,
        #[callback_result] call_result: Result<
            Result<SignatureResponse, String>,
            near_sdk::PromiseError,
        >,
        nonce: U128,
    );
}

#[ext_contract(ext_token)]
pub trait ExtToken {
    fn ft_transfer(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
    ) -> PromiseOrValue<U128>;

    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128>;

    fn ft_metadata(&self) -> FungibleTokenMetadata;

    fn storage_balance_of(&mut self, account_id: Option<AccountId>) -> Option<StorageBalance>;
}

#[ext_contract(ext_signer)]
pub trait ExtSigner {
    fn sign(&mut self, request: SignRequest);
}

#[near(contract_state)]
#[derive(Pausable, Upgradable, PanicOnDefault)]
#[access_control(role_type(Role))]
#[pausable(manager_roles(Role::PauseManager))]
#[upgradable(access_control_roles(
    code_stagers(Role::UpgradableCodeStager, Role::DAO),
    code_deployers(Role::UpgradableCodeDeployer, Role::DAO),
    duration_initializers(Role::DAO),
    duration_update_stagers(Role::DAO),
    duration_update_appliers(Role::DAO),
))]
pub struct Contract {
    pub prover_account: AccountId,
    pub factories: LookupMap<ChainKind, OmniAddress>,
    pub pending_transfers: LookupMap<Nonce, TransferMessage>,
    pub mpc_signer: AccountId,
    pub current_nonce: Nonce,
}

#[near]
impl FungibleTokenReceiver for Contract {
    /// Callback on receiving tokens by this contract.
    /// msg: `Ethereum` address to receive the tokens on.
    #[pause(except(roles(Role::DAO, Role::UnrestrictedDeposit)))]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.current_nonce += 1;
        let event = TransferMessage {
            nonce: self.current_nonce,
            token: env::predecessor_account_id().to_string(),
            amount: amount.0,
            recipient: msg.parse().unwrap(),
            fee: 0,
            sender: OmniAddress::Near(sender_id.to_string()),
        };
        self.pending_transfers.insert(&self.current_nonce, &event);

        PromiseOrValue::Value(U128(0))
    }
}

#[near]
impl Contract {
    #[init]
    pub fn new(prover_account: AccountId, mpc_signer: AccountId, nonce: U128) -> Self {
        let mut contract = Self {
            prover_account,
            factories: LookupMap::new(StorageKey::Factories),
            pending_transfers: LookupMap::new(StorageKey::PendingTransfers),
            mpc_signer,
            current_nonce: nonce.0,
        };

        contract.acl_init_super_admin(near_sdk::env::predecessor_account_id());
        contract.acl_grant_role("DAO".to_owned(), near_sdk::env::predecessor_account_id());
        contract
    }

    pub fn log_metadata(&self, token_id: AccountId) -> Promise {
        ext_token::ext(token_id.clone())
            .with_static_gas(LOG_METADATA_GAS)
            .ft_metadata()
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(LOG_METADATA_CALLBCAK_GAS)
                    .log_metadata_callbcak(token_id),
            )
    }

    #[private]
    #[result_serializer(borsh)]
    pub fn log_metadata_callbcak(
        &self,
        #[callback] metadata: FungibleTokenMetadata,
        token_id: AccountId,
    ) -> Promise {
        let metadata_paylaod = MetadataPayload {
            token: token_id.to_string(),
            name: metadata.name,
            symbol: metadata.symbol,
            decimals: metadata.decimals,
        };

        let payload = near_sdk::env::keccak256_array(&borsh::to_vec(&metadata_paylaod).unwrap());

        ext_signer::ext(self.mpc_signer.clone())
            .with_static_gas(MPC_SIGNING_GAS)
            .with_attached_deposit(env::attached_deposit())
            .sign(SignRequest {
                payload,
                path: "bridge-1".to_owned(),
                key_version: 0,
            })
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn add_factory(&mut self, address: OmniAddress) {
        self.factories.insert(&(&address).into(), &address);
    }

    pub fn update_transfer_fee(&mut self, nonce: U128, fee: UpdateFee) {
        match fee {
            UpdateFee::Fee(fee) => {
                let mut message = self
                    .pending_transfers
                    .get(&nonce.0)
                    .unwrap_or_else(|| env::panic_str("Transfer not exist"));

                require!(
                    OmniAddress::Near(env::predecessor_account_id().to_string()) == message.sender,
                    "Only sender can update fee"
                );

                message.fee = fee.0;
                self.pending_transfers.insert(&nonce.0, &message);
            }
            UpdateFee::Proof(_) => env::panic_str("TODO"),
        }
    }

    #[payable]
    pub fn sign_transfer(&mut self, nonce: U128, relayer: Option<OmniAddress>) -> Promise {
        let transfer_message = self
            .pending_transfers
            .get(&nonce.0)
            .unwrap_or_else(|| env::panic_str("The transfer does not exist"));
        let withdraw_payload = TransferMessagePayload {
            nonce: transfer_message.nonce,
            token: transfer_message.token,
            amount: transfer_message.amount - transfer_message.fee,
            recipient: transfer_message.recipient,
            relayer,
        };

        let payload = near_sdk::env::keccak256_array(&borsh::to_vec(&withdraw_payload).unwrap());

        ext_signer::ext(self.mpc_signer.clone())
            .with_static_gas(MPC_SIGNING_GAS)
            .with_attached_deposit(env::attached_deposit())
            .sign(SignRequest {
                payload,
                path: "bridge-1".to_owned(),
                key_version: 0,
            })
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(SIGN_TRANSFER_CALLBACK_GAS)
                    .sign_transfer_callback(nonce),
            )
    }

    #[private]
    pub fn sign_transfer_callback(
        &mut self,
        #[callback_result] call_result: Result<
            Result<SignatureResponse, String>,
            near_sdk::PromiseError,
        >,
        nonce: U128,
    ) {
        if let Ok(Ok(_)) = call_result {
            let transfer_message = self
                .pending_transfers
                .get(&nonce.0)
                .unwrap_or_else(|| panic!("The transfer does not exist"));

            if transfer_message.fee == 0 {
                self.pending_transfers.remove(&nonce.0);
            }
        }
    }
}
