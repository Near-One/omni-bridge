#![allow(clippy::too_many_arguments)]
use helpers::{PromiseOrPromiseIndexOrValue, SdkExpect};
use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_contract_standards::storage_management::StorageBalance;
use near_plugins::{
    access_control, access_control_any, pause, AccessControlRole, AccessControllable, Pausable,
    Upgradable,
};

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, LookupSet, UnorderedMap};
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;
use near_sdk::{
    env, ext_contract, near, require, serde_json, AccountId, BorshStorageKey, CryptoHash, Gas,
    GasWeight, NearToken, PanicOnDefault, Promise, PromiseError, PromiseOrValue, PromiseResult,
};
use omni_types::btc::{TxOut, UTXOChainConfig};
use omni_types::locker_args::{
    AddDeployedTokenArgs, BindTokenArgs, ClaimFeeArgs, DeployTokenArgs, FinTransferArgs,
    StorageDepositAction,
};
use omni_types::mpc_types::SignatureResponse;
use omni_types::near_events::OmniBridgeEvent;
use omni_types::prover_result::ProverResult;
use omni_types::{
    BasicMetadata, BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, FastTransfer,
    FastTransferId, FastTransferStatus, Fee, InitTransferMsg, MetadataPayload, Nonce, OmniAddress,
    PayloadType, SignRequest, TransferId, TransferIdKind, TransferMessage, TransferMessagePayload,
    UnifiedTransferId, UpdateFee, UtxoFinTransferMsg, H160,
};
use std::collections::HashMap;
use std::str::FromStr;
use storage::{
    Decimals, FastTransferStatusStorage, TransferMessageStorage, TransferMessageStorageValue,
    NEP141_DEPOSIT,
};

mod btc;
mod helpers;
mod migrate;
mod storage;

#[cfg(test)]
mod tests;

const LOG_METADATA_GAS: Gas = Gas::from_tgas(10);
const LOG_METADATA_CALLBACK_GAS: Gas = Gas::from_tgas(260);
const MPC_SIGNING_GAS: Gas = Gas::from_tgas(250);
const SIGN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const SIGN_LOG_METADATA_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const FIN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(250);
const CLAIM_FEE_CALLBACK_GAS: Gas = Gas::from_tgas(50);
const BIND_TOKEN_CALLBACK_GAS: Gas = Gas::from_tgas(25);
const BIND_TOKEN_REFUND_GAS: Gas = Gas::from_tgas(5);
const FT_TRANSFER_CALL_GAS: Gas = Gas::from_tgas(210);
const FT_TRANSFER_GAS: Gas = Gas::from_tgas(5);
const UPDATE_CONTROLLER_GAS: Gas = Gas::from_tgas(250);
const WNEAR_WITHDRAW_GAS: Gas = Gas::from_tgas(5);
const NEAR_WITHDRAW_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const STORAGE_BALANCE_OF_GAS: Gas = Gas::from_tgas(3);
const STORAGE_DEPOSIT_GAS: Gas = Gas::from_tgas(3);
const DEPLOY_TOKEN_CALLBACK_GAS: Gas = Gas::from_tgas(75);
const DEPLOY_TOKEN_GAS: Gas = Gas::from_tgas(50);
const BURN_TOKEN_GAS: Gas = Gas::from_tgas(3);
const MINT_TOKEN_GAS: Gas = Gas::from_tgas(5);
const SET_METADATA_GAS: Gas = Gas::from_tgas(10);
const RESOLVE_FAST_TRANSFER_GAS: Gas = Gas::from_tgas(6);
const UTXO_FIN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const RESOLVE_UTXO_FIN_TRANSFER_GAS: Gas = Gas::from_tgas(3);
const FAST_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(10);
const NO_DEPOSIT: NearToken = NearToken::from_near(0);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const SEND_TOKENS_CALLBACK_GAS: Gas = Gas::from_tgas(15);
const VERIFY_PROOF_GAS: Gas = Gas::from_tgas(20);
const INIT_TRANSFER_RESUME_GAS: Gas = Gas::from_tgas(10);
const SIGN_PATH: &str = "bridge-1";

const PROMISE_REGISTER_ID: u64 = 0;

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    PendingTransfers,
    Factories,
    FinalisedTransfers,
    FinalisedUtxoTransfers,
    TokenIdToAddress,
    AccountsBalances,
    TokenAddressToId,
    TokenDeployerAccounts,
    DeployedTokens,
    DestinationNonces,
    TokenDecimals,
    FastTransfers,
    RegisteredProvers,
    InitTransferPromises,
}

#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    DAO,
    PauseManager,
    UnrestrictedDeposit,
    UpgradableCodeStager,
    UpgradableCodeDeployer,
    MetadataManager,
    UnrestrictedRelayer,
    TokenControllerUpdater,
    NativeFeeRestricted,
    RbfOperator,
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

    fn storage_deposit(
        &mut self,
        account_id: &AccountId,
        registration_only: Option<bool>,
    ) -> Option<StorageBalance>;

    fn storage_balance_of(&mut self, account_id: &AccountId) -> Option<StorageBalance>;

    fn mint(&mut self, account_id: AccountId, amount: U128, msg: Option<String>);

    fn burn(&mut self, amount: U128);

    fn set_metadata(
        &mut self,
        name: Option<String>,
        symbol: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
        decimals: Option<u8>,
        icon: Option<String>,
    );
}

#[near(serializers = [json])]
#[derive(Clone)]
pub struct BtcConfig {
    pub chain_signatures_root_public_key: Option<near_sdk::PublicKey>,
}

#[ext_contract(ext_bridge_token_facory)]
pub trait ExtBridgeTokenFactory {
    fn set_controller_for_tokens(&self, tokens_account_id: Vec<AccountId>);
}

#[ext_contract(ext_signer)]
pub trait ExtSigner {
    fn sign(&mut self, request: SignRequest);
}

#[ext_contract(ext_omni_prover_proxy)]
pub trait Prover {
    fn verify_proof(&self, #[serializer(borsh)] proof: Vec<u8>);
}

#[ext_contract(ext_wnear_token)]
pub trait ExtWNearToken {
    fn near_withdraw(&self, amount: U128);
}

#[ext_contract(ext_deployer)]
pub trait TokenDeployer {
    fn deploy_token(&self, account_id: AccountId, metadata: BasicMetadata) -> Promise;
}

#[ext_contract(ext_utxo_connector)]
pub trait ExtUTXOConnector {
    fn withdraw_rbf(&mut self, original_btc_pending_verify_id: String, output: Vec<TxOut>);
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
    pub factories: LookupMap<ChainKind, OmniAddress>,
    pub pending_transfers: LookupMap<TransferId, TransferMessageStorage>,
    pub finalised_transfers: LookupSet<TransferId>,
    pub finalised_utxo_transfers: LookupSet<UnifiedTransferId>,
    pub fast_transfers: LookupMap<FastTransferId, FastTransferStatusStorage>,
    pub token_id_to_address: LookupMap<(ChainKind, AccountId), OmniAddress>,
    pub token_address_to_id: LookupMap<OmniAddress, AccountId>,
    pub token_decimals: LookupMap<OmniAddress, Decimals>,
    pub deployed_tokens: LookupSet<AccountId>,
    pub token_deployer_accounts: LookupMap<ChainKind, AccountId>,
    pub mpc_signer: AccountId,
    pub current_origin_nonce: Nonce,
    // We maintain a separate nonce for each chain to optimize the storage usage on Solana by reducing the gaps.
    pub destination_nonces: LookupMap<ChainKind, Nonce>,
    pub accounts_balances: LookupMap<AccountId, StorageBalance>,
    pub wnear_account_id: AccountId,
    pub provers: UnorderedMap<ChainKind, AccountId>,
    pub init_transfer_promises: LookupMap<AccountId, CryptoHash>,
    pub utxo_chain_connectors: HashMap<ChainKind, UTXOChainConfig>,
}

#[near]
impl Contract {
    #[pause(except(roles(Role::DAO, Role::UnrestrictedDeposit)))]
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) {
        let token_id = env::predecessor_account_id();
        let parsed_msg: BridgeOnTransferMsg = serde_json::from_str(&msg)
            .or_else(|_| serde_json::from_str(&msg).map(BridgeOnTransferMsg::InitTransfer))
            .sdk_expect("ERR_PARSE_MSG");

        // We can't trust sender_id to pay for storage as it can be spoofed.
        let signer_id = env::signer_account_id();
        let promise_or_promise_index_or_value = match parsed_msg {
            BridgeOnTransferMsg::InitTransfer(init_transfer_msg) => {
                self.init_transfer(sender_id, signer_id, token_id, amount, init_transfer_msg)
            }
            BridgeOnTransferMsg::FastFinTransfer(fast_fin_transfer_msg) => {
                self.fast_fin_transfer(token_id, amount, signer_id, fast_fin_transfer_msg)
            }
            BridgeOnTransferMsg::UtxoFinTransfer(utxo_fin_transfer_msg) => self.utxo_fin_transfer(
                token_id,
                amount,
                &signer_id,
                &sender_id,
                utxo_fin_transfer_msg,
            ),
        };

        promise_or_promise_index_or_value.as_return();
    }

    #[init]
    pub fn new(mpc_signer: AccountId, wnear_account_id: AccountId) -> Self {
        let mut contract = Self {
            factories: LookupMap::new(StorageKey::Factories),
            pending_transfers: LookupMap::new(StorageKey::PendingTransfers),
            finalised_transfers: LookupSet::new(StorageKey::FinalisedTransfers),
            finalised_utxo_transfers: LookupSet::new(StorageKey::FinalisedUtxoTransfers),
            fast_transfers: LookupMap::new(StorageKey::FastTransfers),
            token_id_to_address: LookupMap::new(StorageKey::TokenIdToAddress),
            token_address_to_id: LookupMap::new(StorageKey::TokenAddressToId),
            token_decimals: LookupMap::new(StorageKey::TokenDecimals),
            deployed_tokens: LookupSet::new(StorageKey::DeployedTokens),
            token_deployer_accounts: LookupMap::new(StorageKey::TokenDeployerAccounts),
            mpc_signer,
            current_origin_nonce: 0,
            destination_nonces: LookupMap::new(StorageKey::DestinationNonces),
            accounts_balances: LookupMap::new(StorageKey::AccountsBalances),
            wnear_account_id,
            provers: UnorderedMap::new(StorageKey::RegisteredProvers),
            init_transfer_promises: LookupMap::new(StorageKey::InitTransferPromises),
            utxo_chain_connectors: HashMap::new(),
        };

        contract.acl_init_super_admin(near_sdk::env::predecessor_account_id());
        contract.acl_grant_role(Role::DAO.into(), near_sdk::env::predecessor_account_id());
        contract
    }

    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn log_metadata(&self, token_id: &AccountId) -> Promise {
        ext_token::ext(token_id.clone())
            .with_static_gas(LOG_METADATA_GAS)
            .ft_metadata()
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(LOG_METADATA_CALLBACK_GAS)
                    .with_attached_deposit(env::attached_deposit())
                    .log_metadata_callback(token_id),
            )
    }

    #[private]
    #[result_serializer(borsh)]
    pub fn log_metadata_callback(
        &self,
        #[callback] metadata: FungibleTokenMetadata,
        token_id: &AccountId,
    ) -> Promise {
        require!(
            !metadata.name.is_empty() && !metadata.symbol.is_empty(),
            "ERR_INVALID_METADATA"
        );

        let metadata_payload = MetadataPayload {
            prefix: PayloadType::Metadata,
            token: token_id.to_string(),
            name: metadata.name,
            symbol: metadata.symbol,
            decimals: metadata.decimals,
        };

        let payload = near_sdk::env::keccak256_array(
            &borsh::to_vec(&metadata_payload).sdk_expect("ERR_BORSH"),
        );

        ext_signer::ext(self.mpc_signer.clone())
            .with_static_gas(MPC_SIGNING_GAS)
            .with_attached_deposit(env::attached_deposit())
            .sign(SignRequest {
                payload,
                path: SIGN_PATH.to_owned(),
                key_version: 0,
            })
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(SIGN_LOG_METADATA_CALLBACK_GAS)
                    .sign_log_metadata_callback(metadata_payload),
            )
    }

    #[private]
    #[result_serializer(borsh)]
    pub fn sign_log_metadata_callback(
        &self,
        #[callback_result] call_result: Result<SignatureResponse, PromiseError>,
        #[serializer(borsh)] metadata_payload: MetadataPayload,
    ) {
        if let Ok(signature) = call_result {
            env::log_str(
                &OmniBridgeEvent::LogMetadataEvent {
                    signature,
                    metadata_payload,
                }
                .to_log_string(),
            );
        }
    }

    #[payable]
    #[pause]
    pub fn update_transfer_fee(&mut self, transfer_id: TransferId, fee: UpdateFee) {
        match fee {
            UpdateFee::Fee(fee) => {
                let mut transfer = self.get_transfer_message_storage(transfer_id);

                let current_fee = transfer.message.fee;
                require!(
                    fee.fee >= current_fee.fee && fee.fee < transfer.message.amount,
                    "ERR_INVALID_FEE"
                );

                require!(
                    fee.fee == current_fee.fee
                        || OmniAddress::Near(env::predecessor_account_id())
                            == transfer.message.sender,
                    "Only sender can update token fee"
                );

                let diff_native_fee = fee
                    .native_fee
                    .0
                    .checked_sub(current_fee.native_fee.0)
                    .sdk_expect("ERR_LOWER_FEE");

                require!(
                    NearToken::from_yoctonear(diff_native_fee) == env::attached_deposit(),
                    "ERR_INVALID_ATTACHED_DEPOSIT"
                );

                transfer.message.fee = fee;
                self.insert_raw_transfer(transfer.message.clone(), transfer.owner);

                env::log_str(
                    &OmniBridgeEvent::UpdateFeeEvent {
                        transfer_message: transfer.message,
                    }
                    .to_log_string(),
                );
            }
            UpdateFee::Proof(_) => env::panic_str("TODO"),
        }
    }

    /// # Panics
    ///
    /// This function will panic under the following conditions:
    ///
    /// - If the `borsh::to_vec` serialization of the `TransferMessagePayload` fails.
    /// - If a `fee` is provided and it doesn't match the fee in the stored transfer message.
    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn sign_transfer(
        &mut self,
        transfer_id: TransferId,
        fee_recipient: Option<AccountId>,
        fee: &Option<Fee>,
    ) -> Promise {
        let transfer_message = self.get_transfer_message(transfer_id);

        if let Some(fee) = &fee {
            require!(&transfer_message.fee == fee, "Invalid fee");
        }

        let token_address = self
            .get_token_address(
                transfer_message.get_destination_chain(),
                self.get_token_id(&transfer_message.token),
            )
            .unwrap_or_else(|| env::panic_str("ERR_FAILED_TO_GET_TOKEN_ADDRESS"));

        let decimals = self
            .token_decimals
            .get(&token_address)
            .sdk_expect("ERR_TOKEN_DECIMALS_NOT_FOUND");
        let amount_to_transfer = Self::normalize_amount(
            transfer_message.amount.0 - transfer_message.fee.fee.0,
            decimals,
        );

        require!(amount_to_transfer > 0, "Invalid amount to transfer");

        let transfer_payload = TransferMessagePayload {
            prefix: PayloadType::TransferMessage,
            destination_nonce: transfer_message.destination_nonce,
            transfer_id,
            token_address,
            amount: U128(amount_to_transfer),
            recipient: transfer_message.recipient,
            fee_recipient,
        };

        let payload = near_sdk::env::keccak256_array(
            &borsh::to_vec(&transfer_payload).sdk_expect("ERR_BORSH"),
        );

        ext_signer::ext(self.mpc_signer.clone())
            .with_static_gas(MPC_SIGNING_GAS)
            .with_attached_deposit(env::attached_deposit())
            .sign(SignRequest {
                payload,
                path: SIGN_PATH.to_owned(),
                key_version: 0,
            })
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(SIGN_TRANSFER_CALLBACK_GAS)
                    .sign_transfer_callback(transfer_payload, &transfer_message.fee),
            )
    }

    fn init_transfer(
        &mut self,
        sender_id: AccountId,
        signer_id: AccountId,
        token_id: AccountId,
        amount: U128,
        init_transfer_msg: InitTransferMsg,
    ) -> PromiseOrPromiseIndexOrValue<U128> {
        require!(
            init_transfer_msg.recipient.get_chain() != ChainKind::Near,
            "ERR_INVALID_RECIPIENT_CHAIN"
        );

        self.current_origin_nonce += 1;
        let destination_nonce =
            self.get_next_destination_nonce(init_transfer_msg.recipient.get_chain());

        let transfer_message = TransferMessage {
            origin_nonce: self.current_origin_nonce,
            token: OmniAddress::Near(token_id),
            amount,
            recipient: init_transfer_msg.recipient,
            fee: Fee {
                fee: init_transfer_msg.fee,
                native_fee: init_transfer_msg.native_token_fee,
            },
            sender: OmniAddress::Near(sender_id),
            msg: init_transfer_msg.msg.unwrap_or_default(),
            destination_nonce,
            origin_transfer_id: None,
        };
        require!(
            transfer_message.fee.fee < transfer_message.amount,
            "ERR_INVALID_FEE"
        );

        let required_storage_balance =
            self.required_balance_for_init_transfer_message(transfer_message.clone());

        let message_storage_account_id = transfer_message.calculate_storage_account_id();

        // Choose storage payer or whether to yield execution until storage is available
        if self
            .try_to_transfer_balance_from_message_account(
                &message_storage_account_id,
                NearToken::from_yoctonear(init_transfer_msg.native_token_fee.0),
                &signer_id,
                required_storage_balance,
            )
            .is_ok()
            || (self.has_storage_balance(
                &signer_id,
                required_storage_balance.saturating_add(NearToken::from_yoctonear(
                    init_transfer_msg.native_token_fee.0,
                )),
            ) && (init_transfer_msg.native_token_fee.0 == 0
                || !self.acl_has_role(Role::NativeFeeRestricted.into(), signer_id.clone())))
        {
            PromiseOrPromiseIndexOrValue::Value(
                self.init_transfer_internal(transfer_message, signer_id),
            )
        } else {
            let promise_index = env::promise_yield_create(
                "init_transfer_resume",
                json!({
                    "transfer_message": transfer_message,
                    "message_storage_account_id": message_storage_account_id,
                    "storage_owner": signer_id,
                })
                .to_string()
                .as_bytes(),
                INIT_TRANSFER_RESUME_GAS,
                GasWeight(0),
                PROMISE_REGISTER_ID,
            );

            let yield_id: CryptoHash = env::read_register(PROMISE_REGISTER_ID)
                .sdk_expect("ERR_READ_PROMISE_REGISTER")
                .try_into()
                .sdk_expect("ERR_READ_PROMISE_YIELD_ID");

            let required_storage_balance = self.add_promise(&message_storage_account_id, &yield_id);

            self.update_storage_balance(
                env::current_account_id(),
                required_storage_balance,
                NearToken::from_yoctonear(0),
            );

            env::log_str(&format!(
                "Yield init transfer until storage is available at {message_storage_account_id}"
            ));

            PromiseOrPromiseIndexOrValue::PromiseIndex(promise_index)
        }
    }

    #[private]
    #[allow(clippy::needless_pass_by_value)]
    pub fn init_transfer_resume(
        &mut self,
        transfer_message: TransferMessage,
        message_storage_account_id: AccountId,
        storage_owner: AccountId,
        #[callback_result] response: Result<(), PromiseError>,
    ) -> U128 {
        self.remove_promise(&message_storage_account_id);

        if response.is_ok() {
            if let Err(err) = self.try_to_transfer_balance_from_message_account(
                &message_storage_account_id,
                NearToken::from_yoctonear(transfer_message.fee.native_fee.0),
                &storage_owner,
                self.required_balance_for_init_transfer(Some(transfer_message.msg.clone())),
            ) {
                env::log_str(&format!("Error paying native fee and storage: {err}"));
                return transfer_message.amount;
            }

            self.init_transfer_internal(transfer_message, storage_owner)
        } else {
            env::log_str("Init transfer resume timeout");
            transfer_message.amount
        }
    }

    #[private]
    pub fn sign_transfer_callback(
        &mut self,
        #[callback_result] call_result: Result<SignatureResponse, PromiseError>,
        #[serializer(borsh)] message_payload: TransferMessagePayload,
        #[serializer(borsh)] fee: &Fee,
    ) {
        if let Ok(signature) = call_result {
            if fee.is_zero() {
                self.remove_transfer_message(message_payload.transfer_id);
            }

            env::log_str(
                &OmniBridgeEvent::SignTransferEvent {
                    signature,
                    message_payload,
                }
                .to_log_string(),
            );
        }
    }

    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn fin_transfer(&mut self, #[serializer(borsh)] args: FinTransferArgs) -> Promise {
        require!(
            args.storage_deposit_actions.len() <= 3,
            "Invalid len of accounts for storage deposit"
        );
        let mut main_promise = self.verify_proof(args.chain_kind, args.prover_args);

        let mut attached_deposit = env::attached_deposit();

        for action in &args.storage_deposit_actions {
            main_promise =
                main_promise.and(Self::check_or_pay_ft_storage(action, &mut attached_deposit));
        }

        main_promise.then(
            Self::ext(env::current_account_id())
                .with_attached_deposit(attached_deposit)
                .with_static_gas(FIN_TRANSFER_CALLBACK_GAS)
                .fin_transfer_callback(
                    &args.storage_deposit_actions,
                    env::predecessor_account_id(),
                ),
        )
    }

    #[private]
    #[payable]
    pub fn fin_transfer_callback(
        &mut self,
        #[serializer(borsh)] storage_deposit_actions: &Vec<StorageDepositAction>,
        #[serializer(borsh)] predecessor_account_id: AccountId,
    ) -> PromiseOrValue<Nonce> {
        let Ok(ProverResult::InitTransfer(init_transfer)) = Self::decode_prover_result(0) else {
            env::panic_str("Invalid proof message")
        };
        require!(
            self.factories
                .get(&init_transfer.emitter_address.get_chain())
                == Some(init_transfer.emitter_address),
            "Unknown factory"
        );

        let decimals = self
            .token_decimals
            .get(&init_transfer.token)
            .sdk_expect("ERR_TOKEN_DECIMALS_NOT_FOUND");

        let destination_nonce =
            self.get_next_destination_nonce(init_transfer.recipient.get_chain());
        let transfer_message = TransferMessage {
            origin_nonce: init_transfer.origin_nonce,
            token: init_transfer.token,
            amount: Self::denormalize_amount(init_transfer.amount.0, decimals).into(),
            recipient: init_transfer.recipient,
            fee: Self::denormalize_fee(&init_transfer.fee, decimals),
            sender: init_transfer.sender,
            msg: init_transfer.msg,
            destination_nonce,
            origin_transfer_id: None,
        };

        if let OmniAddress::Near(recipient) = transfer_message.recipient.clone() {
            self.process_fin_transfer_to_near(
                recipient,
                &predecessor_account_id,
                transfer_message,
                storage_deposit_actions,
            )
            .into()
        } else {
            self.process_fin_transfer_to_other_chain(predecessor_account_id, transfer_message);
            PromiseOrValue::Value(destination_nonce)
        }
    }

    fn fast_fin_transfer(
        &mut self,
        token_id: AccountId,
        amount: U128,
        storage_payer: AccountId,
        fast_fin_transfer_msg: FastFinTransferMsg,
    ) -> PromiseOrPromiseIndexOrValue<U128> {
        let origin_token = self
            .get_token_address(
                fast_fin_transfer_msg.transfer_id.origin_chain,
                token_id.clone(),
            )
            .sdk_expect("ERR_TOKEN_NOT_FOUND");

        let decimals = self
            .token_decimals
            .get(&origin_token)
            .sdk_expect("ERR_TOKEN_DECIMALS_NOT_FOUND");

        let denormalized_amount =
            Self::denormalize_amount(fast_fin_transfer_msg.amount.0, decimals);
        let denormalized_fee = Self::denormalize_fee(&fast_fin_transfer_msg.fee, decimals);
        require!(
            denormalized_amount == amount.0 + denormalized_fee.fee.0,
            "ERR_INVALID_FAST_TRANSFER_AMOUNT"
        );

        if self.is_transfer_finalised(&fast_fin_transfer_msg.transfer_id) {
            env::panic_str("ERR_TRANSFER_ALREADY_FINALISED");
        }

        let fast_transfer = FastTransfer {
            token_id: token_id.clone(),
            recipient: fast_fin_transfer_msg.recipient.clone(),
            amount: U128(denormalized_amount),
            fee: denormalized_fee,
            transfer_id: fast_fin_transfer_msg.transfer_id,
            msg: fast_fin_transfer_msg.msg,
        };

        if let OmniAddress::Near(recipient) = fast_fin_transfer_msg.recipient {
            let storage_deposit_amount = fast_fin_transfer_msg
                .storage_deposit_amount
                .map(|amount| amount.0)
                .unwrap_or_default();
            if storage_deposit_amount > 0 {
                self.update_storage_balance(
                    storage_payer.clone(),
                    NearToken::from_yoctonear(storage_deposit_amount),
                    NearToken::from_yoctonear(0),
                );
            }

            let deposit_action = StorageDepositAction {
                account_id: recipient,
                token_id,
                storage_deposit_amount: fast_fin_transfer_msg
                    .storage_deposit_amount
                    .map(|amount| amount.0),
            };

            Self::check_or_pay_ft_storage(
                &deposit_action,
                &mut NearToken::from_yoctonear(storage_deposit_amount),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(
                        FAST_TRANSFER_CALLBACK_GAS.saturating_add(FT_TRANSFER_CALL_GAS),
                    )
                    .fast_fin_transfer_to_near_callback(
                        &fast_transfer,
                        storage_payer,
                        fast_fin_transfer_msg.relayer,
                    ),
            )
            .into()
        } else {
            self.fast_fin_transfer_to_other_chain(
                &fast_transfer,
                storage_payer,
                fast_fin_transfer_msg.relayer,
            );
            self.burn_tokens_if_needed(token_id, amount);
            PromiseOrPromiseIndexOrValue::Value(U128(0))
        }
    }

    #[private]
    pub fn fast_fin_transfer_to_near_callback(
        &mut self,
        #[serializer(borsh)] fast_transfer: &FastTransfer,
        #[serializer(borsh)] storage_payer: AccountId,
        #[serializer(borsh)] relayer_id: AccountId,
    ) -> Promise {
        require!(
            Self::check_storage_balance_result(0),
            "STORAGE_ERR: The transfer recipient is omitted"
        );

        let OmniAddress::Near(recipient) = fast_transfer.recipient.clone() else {
            env::panic_str("ERR_INVALID_STATE")
        };

        let required_balance = self
            .add_fast_transfer(fast_transfer, relayer_id, storage_payer.clone())
            .saturating_add(ONE_YOCTO);
        self.update_storage_balance(
            storage_payer,
            required_balance,
            NearToken::from_yoctonear(0),
        );

        env::log_str(
            &OmniBridgeEvent::FastTransferEvent {
                fast_transfer: fast_transfer.clone(),
                new_transfer_id: None,
            }
            .to_log_string(),
        );

        let amount = U128(fast_transfer.amount.0 - fast_transfer.fee.fee.0);

        self.send_tokens(
            fast_transfer.token_id.clone(),
            recipient,
            amount,
            &fast_transfer.msg,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(RESOLVE_FAST_TRANSFER_GAS)
                .resolve_fast_transfer(
                    fast_transfer.token_id.clone(),
                    &fast_transfer.id(),
                    amount,
                    !fast_transfer.msg.is_empty(),
                ),
        )
    }

    #[private]
    pub fn resolve_fast_transfer(
        &mut self,
        token_id: AccountId,
        fast_transfer_id: &FastTransferId,
        amount: U128,
        is_ft_transfer_call: bool,
    ) -> U128 {
        // Burn the tokens to ensure the locked tokens are not double-minted
        self.burn_tokens_if_needed(token_id, amount);

        if Self::is_refund_required(is_ft_transfer_call) {
            self.remove_fast_transfer(fast_transfer_id);
            amount
        } else {
            U128(0)
        }
    }

    fn fast_fin_transfer_to_other_chain(
        &mut self,
        fast_transfer: &FastTransfer,
        storage_payer: AccountId,
        relayer_id: AccountId,
    ) {
        if fast_transfer.recipient.is_utxo_chain() {
            let btc_account_id = self.get_utxo_chain_token(fast_transfer.recipient.get_chain());
            require!(
                fast_transfer.token_id == btc_account_id,
                "Only BTC can be transferred to the Bitcoin network."
            );
        }

        let mut required_balance =
            self.add_fast_transfer(fast_transfer, relayer_id.clone(), storage_payer.clone());

        let destination_nonce =
            self.get_next_destination_nonce(fast_transfer.recipient.get_chain());
        self.current_origin_nonce += 1;

        let transfer_message = TransferMessage {
            origin_nonce: self.current_origin_nonce,
            token: OmniAddress::Near(fast_transfer.token_id.clone()),
            amount: fast_transfer.amount,
            recipient: fast_transfer.recipient.clone(),
            fee: fast_transfer.fee.clone(),
            sender: OmniAddress::Near(relayer_id),
            msg: fast_transfer.msg.clone(),
            destination_nonce,
            origin_transfer_id: Some(fast_transfer.transfer_id.clone()),
        };
        let new_transfer_id = transfer_message.get_transfer_id();

        required_balance = self
            .add_transfer_message(transfer_message, storage_payer.clone())
            .saturating_add(required_balance);

        env::log_str(
            &OmniBridgeEvent::FastTransferEvent {
                fast_transfer: fast_transfer.clone(),
                new_transfer_id: Some(new_transfer_id),
            }
            .to_log_string(),
        );

        self.update_storage_balance(storage_payer, required_balance, NearToken::from_near(0));
    }

    #[private]
    pub fn utxo_fin_transfer_to_near_callback(
        &mut self,
        token_id: AccountId,
        recipient: AccountId,
        amount: U128,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
        origin_chain: ChainKind,
        storage_owner: &AccountId,
    ) -> Promise {
        self.utxo_fin_transfer_to_near_callback_internal(
            token_id,
            recipient,
            amount,
            utxo_fin_transfer_msg,
            origin_chain,
            storage_owner,
        )
    }

    #[private]
    pub fn resolve_utxo_fin_transfer(
        &mut self,
        token_id: AccountId,
        amount: U128,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
        origin_chain: ChainKind,
        storage_owner: &AccountId,
    ) -> U128 {
        let required_storage_balance = self.add_fin_utxo_transfer(&UnifiedTransferId {
            origin_chain,
            kind: TransferIdKind::Utxo(utxo_fin_transfer_msg.utxo_id.clone()),
        });

        self.update_storage_balance(
            storage_owner.clone(),
            required_storage_balance,
            NearToken::from_yoctonear(0),
        );

        self.resolve_utxo_fin_transfer_internal(
            token_id,
            amount,
            utxo_fin_transfer_msg,
            origin_chain,
            storage_owner,
        )
    }

    #[private]
    pub fn near_withdraw_callback(&self, recipient: AccountId, amount: NearToken) -> Promise {
        match env::promise_result(0) {
            PromiseResult::Successful(_) => Promise::new(recipient).transfer(amount),
            PromiseResult::Failed => env::panic_str("ERR_NEAR_WITHDRAW_FAILED"),
        }
    }

    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn claim_fee(&mut self, #[serializer(borsh)] args: ClaimFeeArgs) -> Promise {
        self.verify_proof(args.chain_kind, args.prover_args).then(
            Self::ext(env::current_account_id())
                .with_attached_deposit(env::attached_deposit())
                .with_static_gas(CLAIM_FEE_CALLBACK_GAS)
                .claim_fee_callback(&env::predecessor_account_id()),
        )
    }

    #[private]
    #[payable]
    pub fn claim_fee_callback(
        &mut self,
        #[serializer(borsh)] predecessor_account_id: &AccountId,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProverResult, PromiseError>,
    ) -> PromiseOrValue<()> {
        let Ok(ProverResult::FinTransfer(fin_transfer)) = call_result else {
            env::panic_str("Invalid proof message")
        };

        let fee_recipient = fin_transfer.fee_recipient.unwrap_or_else(|| {
            env::panic_str("ERR_FEE_RECIPIENT_NOT_SET_OR_EMPTY");
        });

        require!(
            fee_recipient == *predecessor_account_id,
            "ERR_ONLY_FEE_RECIPIENT_CAN_CLAIM"
        );
        require!(
            self.factories
                .get(&fin_transfer.emitter_address.get_chain())
                .as_ref()
                == Some(&fin_transfer.emitter_address),
            "ERR_UNKNOWN_FACTORY"
        );

        let message = self.remove_transfer_message(fin_transfer.transfer_id);

        if let Some(origin_transfer_id) = message.origin_transfer_id.clone() {
            let mut fast_transfer =
                FastTransfer::from_transfer(message.clone(), self.get_token_id(&message.token));
            fast_transfer.transfer_id = origin_transfer_id;

            if let Some(fast_transfer_status) = self.get_fast_transfer_status(&fast_transfer.id()) {
                // For fast transfers we need to wait for finalization of the first leg (Origin chain -> Near) before allowing fee claim.
                // This confirms that fast transfer was executed with correct parameters.
                // Othewise malicious relayer can create a fast transfer with arbitrary high fee and claim it here.
                if fast_transfer_status.finalised {
                    self.remove_fast_transfer(&fast_transfer.id());
                } else {
                    env::panic_str("ERR_FAST_TRANSFER_NOT_FINALISED");
                }
            }
        }

        let token = self.get_token_id(&message.token);
        let token_address = self
            .get_token_address(message.get_destination_chain(), token.clone())
            .unwrap_or_else(|| env::panic_str("ERR_FAILED_TO_GET_TOKEN_ADDRESS"));

        let denormalized_amount = Self::denormalize_amount(
            fin_transfer.amount.0,
            self.token_decimals
                .get(&token_address)
                .sdk_expect("ERR_TOKEN_DECIMALS_NOT_FOUND"),
        );
        let fee = message.amount.0 - denormalized_amount;

        self.send_fee_internal(&message, fee_recipient, fee)
    }

    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn deploy_token(&mut self, #[serializer(borsh)] args: DeployTokenArgs) -> Promise {
        self.verify_proof(args.chain_kind, args.prover_args).then(
            Self::ext(env::current_account_id())
                .with_attached_deposit(NO_DEPOSIT)
                .with_static_gas(DEPLOY_TOKEN_CALLBACK_GAS)
                .deploy_token_callback(near_sdk::env::attached_deposit()),
        )
    }

    #[private]
    pub fn deploy_token_callback(
        &mut self,
        attached_deposit: NearToken,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProverResult, PromiseError>,
    ) -> Promise {
        let Ok(ProverResult::LogMetadata(metadata)) = call_result else {
            env::panic_str("ERR_INVALID_PROOF");
        };

        let chain = metadata.emitter_address.get_chain();
        require!(
            self.factories.get(&chain) == Some(metadata.emitter_address),
            "ERR_UNKNOWN_FACTORY"
        );

        self.deploy_token_internal(
            chain,
            &metadata.token_address,
            BasicMetadata {
                name: metadata.name,
                symbol: metadata.symbol,
                decimals: metadata.decimals,
            },
            attached_deposit,
        )
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn deploy_native_token(
        &mut self,
        chain_kind: ChainKind,
        name: String,
        symbol: String,
        decimals: u8,
    ) -> Promise {
        self.deploy_token_internal(
            chain_kind,
            &OmniAddress::new_zero(chain_kind)
                .unwrap_or_else(|_| env::panic_str("ERR_FAILED_TO_GET_ZERO_ADDRESS")),
            BasicMetadata {
                name,
                symbol,
                decimals,
            },
            env::attached_deposit(),
        )
    }

    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn bind_token(&mut self, #[serializer(borsh)] args: BindTokenArgs) -> Promise {
        self.verify_proof(args.chain_kind, args.prover_args)
            .then(
                Self::ext(env::current_account_id())
                    .with_attached_deposit(NO_DEPOSIT)
                    .with_static_gas(BIND_TOKEN_CALLBACK_GAS)
                    .bind_token_callback(near_sdk::env::attached_deposit()),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_attached_deposit(env::attached_deposit())
                    .with_static_gas(BIND_TOKEN_REFUND_GAS)
                    .bind_token_refund(near_sdk::env::predecessor_account_id()),
            )
    }

    #[private]
    pub fn bind_token_callback(
        &mut self,
        attached_deposit: NearToken,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProverResult, PromiseError>,
    ) -> NearToken {
        let Ok(ProverResult::DeployToken(deploy_token)) = call_result else {
            env::panic_str("ERROR: Invalid proof message");
        };

        require!(
            self.factories
                .get(&deploy_token.emitter_address.get_chain())
                == Some(deploy_token.emitter_address),
            "Unknown factory"
        );

        let storage_usage = env::storage_usage();

        self.add_token(
            &deploy_token.token,
            &deploy_token.token_address,
            deploy_token.decimals,
            deploy_token.origin_decimals,
        );

        let required_deposit = env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into());

        require!(
            attached_deposit >= required_deposit,
            "ERROR: The deposit is not sufficient to cover the storage."
        );

        env::log_str(
            &OmniBridgeEvent::BindTokenEvent {
                token_id: deploy_token.token,
                token_address: deploy_token.token_address,
                decimals: deploy_token.decimals,
                origin_decimals: deploy_token.origin_decimals,
            }
            .to_log_string(),
        );

        attached_deposit.saturating_sub(required_deposit)
    }

    #[private]
    #[payable]
    pub fn bind_token_refund(
        &mut self,
        predecessor_account_id: AccountId,
        #[callback_result] call_result: Result<NearToken, PromiseError>,
    ) {
        let refund_amount = call_result.unwrap_or_else(|_| env::attached_deposit());
        Self::refund(predecessor_account_id, refund_amount);
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn finish_withdraw_v2(
        &mut self,
        #[serializer(borsh)] sender_id: &AccountId,
        #[serializer(borsh)] amount: u128,
        #[serializer(borsh)] recipient: String,
    ) {
        let token_id = env::predecessor_account_id();
        require!(self.deployed_tokens.contains(&token_id));

        self.current_origin_nonce += 1;
        let destination_nonce = self.get_next_destination_nonce(ChainKind::Eth);

        let transfer_message = TransferMessage {
            origin_nonce: self.current_origin_nonce,
            token: OmniAddress::Near(token_id),
            amount: U128(amount),
            recipient: OmniAddress::Eth(
                H160::from_str(&recipient).sdk_expect("Error on recipient parsing"),
            ),
            fee: Fee {
                fee: U128(0),
                native_fee: U128(0),
            },
            sender: OmniAddress::Near(sender_id.clone()),
            msg: String::new(),
            destination_nonce,
            origin_transfer_id: None,
        };

        let required_storage_balance =
            self.add_transfer_message(transfer_message.clone(), sender_id.clone());

        self.update_storage_balance(
            env::current_account_id(),
            required_storage_balance,
            NearToken::from_yoctonear(0),
        );

        env::log_str(&OmniBridgeEvent::InitTransferEvent { transfer_message }.to_log_string());
    }

    pub fn get_token_address(
        &self,
        chain_kind: ChainKind,
        token: AccountId,
    ) -> Option<OmniAddress> {
        self.token_id_to_address.get(&(chain_kind, token))
    }

    pub fn get_token_id(&self, address: &OmniAddress) -> AccountId {
        if let OmniAddress::Near(token_account_id) = address {
            token_account_id.clone()
        } else {
            self.token_address_to_id
                .get(address)
                .sdk_expect("ERR_TOKEN_NOT_REGISTERED")
        }
    }

    pub fn get_bridged_token(
        &self,
        address: &OmniAddress,
        chain: ChainKind,
    ) -> Option<OmniAddress> {
        match (address, chain) {
            // NEAR -> NEAR case
            (OmniAddress::Near(near_id), ChainKind::Near) => {
                Some(OmniAddress::Near(near_id.clone()))
            }
            // NEAR -> foreign chain
            (OmniAddress::Near(near_id), _) => {
                self.token_id_to_address.get(&(chain, near_id.clone()))
            }
            // foreign chain -> NEAR
            (foreign_addr, ChainKind::Near) => self
                .token_address_to_id
                .get(foreign_addr)
                .map(OmniAddress::Near),
            // foreign chain -> foreign chain
            (foreign_addr, _) => {
                // First get the NEAR token ID
                let near_id = self.token_address_to_id.get(foreign_addr)?;
                // Then look up the address on target chain
                self.token_id_to_address.get(&(chain, near_id))
            }
        }
    }

    pub fn get_native_token_id(&self, chain: ChainKind) -> AccountId {
        let native_token_address =
            OmniAddress::new_zero(chain).sdk_expect("ERR_FAILED_TO_GET_ZERO_ADDRESS");

        self.get_token_id(&native_token_address)
    }

    pub fn get_transfer_message(&self, transfer_id: TransferId) -> TransferMessage {
        self.pending_transfers
            .get(&transfer_id)
            .map(storage::TransferMessageStorage::into_main)
            .map(|m| m.message)
            .sdk_expect("The transfer does not exist")
    }

    pub fn get_transfer_message_storage(
        &self,
        transfer_id: TransferId,
    ) -> TransferMessageStorageValue {
        self.pending_transfers
            .get(&transfer_id)
            .map(storage::TransferMessageStorage::into_main)
            .sdk_expect("The transfer does not exist")
    }

    pub fn is_transfer_finalised(&self, transfer_id: &UnifiedTransferId) -> bool {
        match transfer_id.kind {
            TransferIdKind::Nonce(nonce) => self.finalised_transfers.contains(&TransferId {
                origin_chain: transfer_id.origin_chain,
                origin_nonce: nonce,
            }),
            TransferIdKind::Utxo(_) => self.finalised_utxo_transfers.contains(transfer_id),
        }
    }

    pub fn get_fast_transfer_status(
        &self,
        fast_transfer_id: &FastTransferId,
    ) -> Option<FastTransferStatus> {
        self.fast_transfers
            .get(fast_transfer_id)
            .map(storage::FastTransferStatusStorage::into_main)
    }

    pub fn is_fast_transfer_finalised(&self, fast_transfer_id: &FastTransferId) -> bool {
        self.fast_transfers
            .get(fast_transfer_id)
            .map(storage::FastTransferStatusStorage::into_main)
            .is_some_and(|status| status.finalised)
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn add_factory(&mut self, address: OmniAddress) {
        self.factories.insert(&(&address).into(), &address);
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn add_token_deployer(&mut self, chain: ChainKind, account_id: AccountId) {
        self.token_deployer_accounts.insert(&chain, &account_id);
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn transfer_token_as_dao(
        &mut self,
        token: AccountId,
        amount: U128,
        recipient: AccountId,
        msg: Option<String>,
    ) -> Promise {
        if let Some(msg) = msg {
            ext_token::ext(token)
                .with_attached_deposit(ONE_YOCTO)
                .with_static_gas(FT_TRANSFER_CALL_GAS)
                .ft_transfer_call(recipient, amount, None, msg)
        } else {
            ext_token::ext(token)
                .with_attached_deposit(ONE_YOCTO)
                .with_static_gas(FT_TRANSFER_GAS)
                .ft_transfer(recipient, amount, None)
        }
    }

    #[access_control_any(roles(Role::DAO))]
    #[payable]
    pub fn add_deployed_tokens(&mut self, tokens: Vec<AddDeployedTokenArgs>) {
        require!(
            env::attached_deposit()
                >= NEP141_DEPOSIT.saturating_mul(tokens.len().try_into().sdk_expect("ERR_CAST")),
            "ERR_NOT_ENOUGH_ATTACHED_DEPOSIT"
        );

        for token_info in tokens {
            self.deployed_tokens.insert(&token_info.token_id);
            self.add_token(
                &token_info.token_id,
                &token_info.token_address,
                token_info.decimals,
                token_info.decimals,
            );
            ext_token::ext(token_info.token_id)
                .with_static_gas(STORAGE_DEPOSIT_GAS)
                .with_attached_deposit(NEP141_DEPOSIT)
                .storage_deposit(&env::current_account_id(), Some(true));
        }
    }

    #[access_control_any(roles(Role::DAO, Role::MetadataManager))]
    pub fn set_token_metadata(
        &mut self,
        address: OmniAddress,
        name: Option<String>,
        symbol: Option<String>,
        icon: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
    ) -> Promise {
        let token = self.get_token_id(&address);
        require!(self.deployed_tokens.contains(&token));

        let decimals = self
            .token_decimals
            .get(&address)
            .sdk_expect("ERR_TOKEN_DECIMALS_NOT_FOUND")
            .decimals;

        ext_token::ext(token)
            .with_static_gas(SET_METADATA_GAS)
            .set_metadata(
                name,
                symbol,
                reference,
                reference_hash,
                Some(decimals),
                icon,
            )
    }

    pub fn get_current_destination_nonce(&self, chain_kind: ChainKind) -> Nonce {
        self.destination_nonces.get(&chain_kind).unwrap_or_default()
    }

    pub fn get_mpc_account(&self) -> AccountId {
        self.mpc_signer.clone()
    }

    pub fn get_token_decimals(&self, address: &OmniAddress) -> Option<Decimals> {
        self.token_decimals.get(address)
    }

    #[access_control_any(roles(Role::DAO, Role::TokenControllerUpdater))]
    pub fn update_tokens_controller(
        &self,
        factory_account_id: AccountId,
        tokens_accounts_id: Vec<AccountId>,
    ) {
        ext_bridge_token_facory::ext(factory_account_id)
            .with_static_gas(UPDATE_CONTROLLER_GAS)
            .set_controller_for_tokens(tokens_accounts_id);
    }

    #[private]
    pub fn fin_transfer_send_tokens_callback(
        &mut self,
        #[serializer(borsh)] transfer_message: TransferMessage,
        #[serializer(borsh)] fee_recipient: &AccountId,
        #[serializer(borsh)] is_ft_transfer_call: bool,
        #[serializer(borsh)] storage_owner: &AccountId,
    ) {
        let token = self.get_token_id(&transfer_message.token);

        if Self::is_refund_required(is_ft_transfer_call) {
            self.burn_tokens_if_needed(
                token,
                U128(transfer_message.amount.0 - transfer_message.fee.fee.0),
            );
            self.remove_fin_transfer(&transfer_message.get_transfer_id(), storage_owner);

            env::log_str(
                &OmniBridgeEvent::FailedFinTransferEvent { transfer_message }.to_log_string(),
            );
        } else {
            // Send fee to the fee recipient
            if transfer_message.fee.fee.0 > 0 {
                if self.deployed_tokens.contains(&token) {
                    ext_token::ext(token).with_static_gas(MINT_TOKEN_GAS).mint(
                        fee_recipient.clone(),
                        transfer_message.fee.fee,
                        None,
                    );
                } else {
                    ext_token::ext(token)
                        .with_attached_deposit(ONE_YOCTO)
                        .with_static_gas(FT_TRANSFER_GAS)
                        .ft_transfer(fee_recipient.clone(), transfer_message.fee.fee, None);
                }
            }

            if transfer_message.fee.native_fee.0 > 0 {
                let native_token_id = self.get_native_token_id(transfer_message.get_origin_chain());

                ext_token::ext(native_token_id)
                    .with_static_gas(MINT_TOKEN_GAS)
                    .mint(fee_recipient.clone(), transfer_message.fee.native_fee, None);
            }

            env::log_str(&OmniBridgeEvent::FinTransferEvent { transfer_message }.to_log_string());
        }
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn add_prover(&mut self, chain: ChainKind, account_id: AccountId) {
        self.provers.insert(&chain, &account_id);
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn remove_prover(&mut self, chain: ChainKind) {
        self.provers.remove(&chain);
    }

    #[must_use]
    pub fn get_provers(&self) -> Vec<(ChainKind, AccountId)> {
        self.provers.iter().collect()
    }

    #[must_use]
    pub fn get_utxo_chain_connectors(&self) -> Vec<(ChainKind, UTXOChainConfig)> {
        self.utxo_chain_connectors.clone().into_iter().collect()
    }

    pub fn get_utxo_chain_by_token(&self, token: &AccountId) -> Option<ChainKind> {
        for (chain, config) in &self.utxo_chain_connectors {
            if &config.token_id == token {
                return Some(*chain);
            }
        }
        None
    }
}

impl Contract {
    fn is_refund_required(is_ft_transfer_call: bool) -> bool {
        if is_ft_transfer_call {
            match env::promise_result(0) {
                PromiseResult::Successful(value) => {
                    if let Ok(amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                        // Normal case: refund if the used token amount is zero
                        // The amount can be zero if the `ft_on_transfer` in the receiver contract returns an amount instead of `0`, or if it panics.
                        amount.0 == 0
                    } else {
                        // Unexpected case: don't refund
                        false
                    }
                }
                // Unexpected case: don't refund
                PromiseResult::Failed => false,
            }
        } else {
            // Not ft_transfer_call: don't refund
            false
        }
    }

    fn burn_tokens_if_needed(&self, token: AccountId, amount: U128) {
        if self.deployed_tokens.contains(&token) {
            ext_token::ext(token)
                .with_static_gas(BURN_TOKEN_GAS)
                .burn(amount);
        }
    }

    fn get_next_destination_nonce(&mut self, chain_kind: ChainKind) -> Nonce {
        if chain_kind == ChainKind::Near {
            return 0;
        }

        let mut payload_nonce = self.destination_nonces.get(&chain_kind).unwrap_or_default();

        payload_nonce += 1;

        self.destination_nonces.insert(&chain_kind, &payload_nonce);

        payload_nonce
    }

    fn init_transfer_internal(
        &mut self,
        transfer_message: TransferMessage,
        storage_owner: AccountId,
    ) -> U128 {
        let required_storage_balance = self
            .add_transfer_message(transfer_message.clone(), storage_owner.clone())
            .saturating_add(NearToken::from_yoctonear(transfer_message.fee.native_fee.0));

        if self
            .try_update_storage_balance(
                storage_owner,
                required_storage_balance,
                NearToken::from_yoctonear(0),
            )
            .is_err()
        {
            return transfer_message.amount;
        }

        if let OmniAddress::Near(token_id) = transfer_message.token.clone() {
            self.burn_tokens_if_needed(token_id, transfer_message.amount);
        } else {
            return transfer_message.amount;
        }

        env::log_str(&OmniBridgeEvent::InitTransferEvent { transfer_message }.to_log_string());
        U128(0)
    }

    #[allow(clippy::too_many_lines, clippy::ptr_arg)]
    fn process_fin_transfer_to_near(
        &mut self,
        recipient: AccountId,
        predecessor_account_id: &AccountId,
        transfer_message: TransferMessage,
        storage_deposit_actions: &Vec<StorageDepositAction>,
    ) -> Promise {
        let mut required_balance = self.add_fin_transfer(&transfer_message.get_transfer_id());

        let token = self.get_token_id(&transfer_message.token);

        // If fast transfer happened, change recipient and fee recipient to the relayer that executed fast transfer
        let fast_transfer = FastTransfer::from_transfer(transfer_message.clone(), token.clone());
        let (recipient, msg, fee_recipient) =
            match self.get_fast_transfer_status(&fast_transfer.id()) {
                Some(status) => {
                    require!(!status.finalised, "ERR_FAST_TRANSFER_ALREADY_FINALISED");
                    self.remove_fast_transfer(&fast_transfer.id());
                    (status.relayer.clone(), String::new(), status.relayer)
                }
                None => (
                    recipient,
                    transfer_message.msg.clone(),
                    predecessor_account_id.clone(),
                ),
            };

        let mut storage_deposit_action_index: usize = 0;
        require!(
            Self::check_storage_balance_result(
                (storage_deposit_action_index + 1)
                    .try_into()
                    .sdk_expect("ERR_CAST")
            ) && storage_deposit_actions[storage_deposit_action_index].account_id == recipient
                && storage_deposit_actions[storage_deposit_action_index].token_id == token,
            "STORAGE_ERR: The transfer recipient is omitted"
        );
        storage_deposit_action_index += 1;

        // One yoctoNear is required to send tokens to the recipient
        required_balance = required_balance.saturating_add(ONE_YOCTO);

        if transfer_message.fee.fee.0 > 0 {
            require!(
                Self::check_storage_balance_result(
                    (storage_deposit_action_index + 1)
                        .try_into()
                        .sdk_expect("ERR_CAST")
                ) && storage_deposit_actions[storage_deposit_action_index].account_id
                    == fee_recipient
                    && storage_deposit_actions[storage_deposit_action_index].token_id == token,
                "STORAGE_ERR: The fee recipient is omitted"
            );
            storage_deposit_action_index += 1;

            required_balance = required_balance.saturating_add(ONE_YOCTO);
        }

        if transfer_message.fee.native_fee.0 > 0 {
            let native_token_id = self.get_native_token_id(transfer_message.get_origin_chain());

            require!(
                Self::check_storage_balance_result(
                    (storage_deposit_action_index + 1)
                        .try_into()
                        .sdk_expect("ERR_CAST")
                ) && storage_deposit_actions[storage_deposit_action_index].account_id
                    == fee_recipient
                    && storage_deposit_actions[storage_deposit_action_index].token_id
                        == native_token_id,
                "STORAGE_ERR: The native fee recipient is omitted"
            );
        }

        self.update_storage_balance(
            predecessor_account_id.clone(),
            required_balance,
            env::attached_deposit(),
        );

        let amount_to_transfer = U128(transfer_message.amount.0 - transfer_message.fee.fee.0);
        self.send_tokens(token.clone(), recipient, amount_to_transfer, &msg)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(SEND_TOKENS_CALLBACK_GAS)
                    .fin_transfer_send_tokens_callback(
                        transfer_message,
                        &fee_recipient,
                        !msg.is_empty(),
                        predecessor_account_id,
                    ),
            )
    }

    fn process_fin_transfer_to_other_chain(
        &mut self,
        predecessor_account_id: AccountId,
        transfer_message: TransferMessage,
    ) {
        let mut required_balance = self.add_fin_transfer(&transfer_message.get_transfer_id());
        let token = self.get_token_id(&transfer_message.token);

        if transfer_message.recipient.is_utxo_chain() {
            let btc_account_id = self.get_utxo_chain_token(transfer_message.recipient.get_chain());
            require!(
                token == btc_account_id,
                "Only BTC can be transferred to the Bitcoin network."
            );
        }

        let fast_transfer = FastTransfer::from_transfer(transfer_message.clone(), token.clone());
        let recipient = match self.get_fast_transfer_status(&fast_transfer.id()) {
            Some(status) => {
                require!(!status.finalised, "ERR_FAST_TRANSFER_ALREADY_FINALISED");
                Some(status.relayer)
            }
            None => None,
        };

        // If fast transfer happened, send tokens to the relayer that executed fast transfer
        if let Some(relayer) = recipient {
            self.send_tokens(
                token,
                relayer,
                U128(transfer_message.amount.0 - transfer_message.fee.fee.0),
                "",
            );
            self.mark_fast_transfer_as_finalised(&fast_transfer.id());
        } else {
            required_balance = self
                .add_transfer_message(transfer_message.clone(), predecessor_account_id.clone())
                .saturating_add(required_balance);
        }

        self.update_storage_balance(
            predecessor_account_id,
            required_balance,
            env::attached_deposit(),
        );

        env::log_str(&OmniBridgeEvent::FinTransferEvent { transfer_message }.to_log_string());
    }

    fn send_tokens(
        &self,
        token: AccountId,
        recipient: AccountId,
        amount: U128,
        msg: &str,
    ) -> Promise {
        let is_deployed_token = self.deployed_tokens.contains(&token);

        if token == self.wnear_account_id && msg.is_empty() {
            // Unwrap wNEAR and transfer NEAR tokens
            ext_wnear_token::ext(self.wnear_account_id.clone())
                .with_static_gas(WNEAR_WITHDRAW_GAS)
                .with_attached_deposit(ONE_YOCTO)
                .near_withdraw(amount)
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(NEAR_WITHDRAW_CALLBACK_GAS)
                        .near_withdraw_callback(recipient, NearToken::from_yoctonear(amount.0)),
                )
        } else if is_deployed_token {
            let deposit = if msg.is_empty() {
                NO_DEPOSIT
            } else {
                ONE_YOCTO
            };
            ext_token::ext(token)
                .with_attached_deposit(deposit)
                .with_static_gas(MINT_TOKEN_GAS.saturating_add(FT_TRANSFER_CALL_GAS))
                .mint(
                    recipient,
                    amount,
                    (!msg.is_empty()).then(|| msg.to_string()),
                )
        } else if msg.is_empty() {
            ext_token::ext(token)
                .with_attached_deposit(ONE_YOCTO)
                .with_static_gas(FT_TRANSFER_GAS)
                .ft_transfer(recipient, amount, None)
        } else {
            ext_token::ext(token)
                .with_attached_deposit(ONE_YOCTO)
                .with_static_gas(FT_TRANSFER_CALL_GAS)
                .ft_transfer_call(recipient, amount, None, msg.to_string())
        }
    }

    fn check_or_pay_ft_storage(
        action: &StorageDepositAction,
        attached_deposit: &mut NearToken,
    ) -> Promise {
        action.storage_deposit_amount.map_or_else(
            || {
                ext_token::ext(action.token_id.clone())
                    .with_static_gas(STORAGE_BALANCE_OF_GAS)
                    .with_attached_deposit(NO_DEPOSIT)
                    .storage_balance_of(&action.account_id)
            },
            |storage_deposit_amount| {
                let storage_deposit_amount = NearToken::from_yoctonear(storage_deposit_amount);

                *attached_deposit = attached_deposit
                    .checked_sub(storage_deposit_amount)
                    .sdk_expect("The attached deposit is less than required");

                ext_token::ext(action.token_id.clone())
                    .with_static_gas(STORAGE_DEPOSIT_GAS)
                    .with_attached_deposit(storage_deposit_amount)
                    .storage_deposit(&action.account_id, Some(true))
            },
        )
    }

    fn check_storage_balance_result(result_idx: u64) -> bool {
        if result_idx >= env::promise_results_count() {
            return false;
        }
        match env::promise_result(result_idx) {
            PromiseResult::Successful(data) => {
                serde_json::from_slice::<Option<StorageBalance>>(&data)
                    .ok()
                    .flatten()
                    .is_some()
            }
            PromiseResult::Failed => false,
        }
    }

    fn decode_prover_result(result_idx: u64) -> Result<ProverResult, PromiseError> {
        match env::promise_result(result_idx) {
            PromiseResult::Successful(data) => {
                Ok(ProverResult::try_from_slice(&data).sdk_expect("Invalid proof"))
            }
            PromiseResult::Failed => Err(PromiseError::Failed),
        }
    }

    fn insert_raw_transfer(
        &mut self,
        transfer_message: TransferMessage,
        message_owner: AccountId,
    ) -> Option<Vec<u8>> {
        self.pending_transfers.insert_raw(
            &borsh::to_vec(&transfer_message.get_transfer_id()).sdk_expect("ERR_BORSH"),
            &TransferMessageStorage::encode_borsh(transfer_message, message_owner)
                .sdk_expect("ERR_BORSH"),
        )
    }

    fn add_transfer_message(
        &mut self,
        transfer_message: TransferMessage,
        message_owner: AccountId,
    ) -> NearToken {
        let storage_usage = env::storage_usage();
        require!(
            self.insert_raw_transfer(transfer_message, message_owner,)
                .is_none(),
            "ERR_KEY_EXIST"
        );
        env::storage_byte_cost().saturating_mul((env::storage_usage() - storage_usage).into())
    }

    fn remove_transfer_message(&mut self, transfer_id: TransferId) -> TransferMessage {
        let storage_usage = env::storage_usage();
        let transfer = self
            .pending_transfers
            .remove(&transfer_id)
            .map(storage::TransferMessageStorage::into_main)
            .sdk_expect("ERR_TRANSFER_NOT_EXIST");

        let refund =
            env::storage_byte_cost().saturating_mul((storage_usage - env::storage_usage()).into());

        if let Some(mut storage) = self.accounts_balances.get(&transfer.owner) {
            storage.available = storage.available.saturating_add(refund);
            self.accounts_balances.insert(&transfer.owner, &storage);
        }

        transfer.message
    }

    fn add_fin_transfer(&mut self, transfer_id: &TransferId) -> NearToken {
        let storage_usage = env::storage_usage();
        require!(
            self.finalised_transfers.insert(transfer_id),
            "The transfer is already finalised"
        );
        env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into())
    }

    fn add_fin_utxo_transfer(&mut self, transfer_id: &UnifiedTransferId) -> NearToken {
        let storage_usage = env::storage_usage();
        require!(
            self.finalised_utxo_transfers.insert(transfer_id),
            "The UTXO transfer is already finalised"
        );
        env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into())
    }

    fn add_fast_transfer(
        &mut self,
        fast_transfer: &FastTransfer,
        relayer: AccountId,
        storage_owner: AccountId,
    ) -> NearToken {
        let storage_usage = env::storage_usage();
        require!(
            self.fast_transfers
                .insert(
                    &fast_transfer.id(),
                    &FastTransferStatusStorage::V0(FastTransferStatus {
                        relayer,
                        storage_owner,
                        finalised: false,
                    }),
                )
                .is_none(),
            "Fast transfer is already performed"
        );
        env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into())
    }

    fn mark_fast_transfer_as_finalised(&mut self, fast_transfer_id: &FastTransferId) {
        let mut status = self
            .get_fast_transfer_status(fast_transfer_id)
            .sdk_expect("ERR_FAST_TRANSFER_NOT_FOUND");
        status.finalised = true;
        self.fast_transfers
            .insert(fast_transfer_id, &FastTransferStatusStorage::V0(status));
    }

    fn remove_fast_transfer(&mut self, fast_transfer_id: &FastTransferId) {
        let storage_usage = env::storage_usage();
        let fast_transfer = self
            .fast_transfers
            .remove(fast_transfer_id)
            .map(storage::FastTransferStatusStorage::into_main)
            .sdk_expect("ERR_TRANSFER_NOT_EXIST");

        let refund =
            env::storage_byte_cost().saturating_mul((storage_usage - env::storage_usage()).into());

        if let Some(mut storage) = self.accounts_balances.get(&fast_transfer.storage_owner) {
            storage.available = storage.available.saturating_add(refund);
            self.accounts_balances
                .insert(&fast_transfer.storage_owner, &storage);
        }
    }

    fn add_promise(&mut self, promise_id: &AccountId, yield_id: &CryptoHash) -> NearToken {
        let storage_usage = env::storage_usage();
        require!(
            self.init_transfer_promises
                .insert(promise_id, yield_id)
                .is_none(),
            "ERR_KEY_EXIST"
        );
        env::storage_byte_cost().saturating_mul((env::storage_usage() - storage_usage).into())
    }

    fn remove_promise(&mut self, promise_id: &AccountId) {
        let storage_usage = env::storage_usage();
        self.init_transfer_promises.remove(promise_id);

        let refund =
            env::storage_byte_cost().saturating_mul((storage_usage - env::storage_usage()).into());

        if let Some(mut storage) = self.accounts_balances.get(&env::current_account_id()) {
            storage.available = storage.available.saturating_add(refund);
            self.accounts_balances
                .insert(&env::current_account_id(), &storage);
        }
    }

    fn remove_fin_transfer(&mut self, transfer_id: &TransferId, storage_owner: &AccountId) {
        let storage_usage = env::storage_usage();
        self.finalised_transfers.remove(transfer_id);

        let refund =
            env::storage_byte_cost().saturating_mul((storage_usage - env::storage_usage()).into());

        if let Some(mut storage) = self.accounts_balances.get(storage_owner) {
            storage.available = storage.available.saturating_add(refund);
            self.accounts_balances.insert(storage_owner, &storage);
        }
    }

    fn remove_fin_utxo_transfer(
        &mut self,
        transfer_id: &UnifiedTransferId,
        storage_owner: &AccountId,
    ) {
        let storage_usage = env::storage_usage();
        self.finalised_utxo_transfers.remove(transfer_id);

        let refund =
            env::storage_byte_cost().saturating_mul((storage_usage - env::storage_usage()).into());

        if let Some(mut storage) = self.accounts_balances.get(storage_owner) {
            storage.available = storage.available.saturating_add(refund);
            self.accounts_balances.insert(storage_owner, &storage);
        }
    }

    fn update_storage_balance(
        &mut self,
        account_id: AccountId,
        required_balance: NearToken,
        attached_deposit: NearToken,
    ) {
        self.try_update_storage_balance(account_id, required_balance, attached_deposit)
            .unwrap_or_else(|err| env::panic_str(&err));
    }

    fn try_update_storage_balance(
        &mut self,
        account_id: AccountId,
        required_balance: NearToken,
        attached_deposit: NearToken,
    ) -> Result<(), String> {
        if attached_deposit >= required_balance {
            Self::refund(
                account_id,
                attached_deposit.saturating_sub(required_balance),
            );
            Ok(())
        } else {
            let required_balance = required_balance.saturating_sub(attached_deposit);

            let Some(mut storage_balance) = self.accounts_balances.get(&account_id) else {
                return Err(format!("Account {account_id} is not registered"));
            };

            if storage_balance.available >= required_balance {
                storage_balance.available =
                    storage_balance.available.saturating_sub(required_balance);
                self.accounts_balances.insert(&account_id, &storage_balance);

                Ok(())
            } else {
                Err(format!(
                    "Not enough storage deposited, required: {}, available: {}",
                    required_balance, storage_balance.available
                ))
            }
        }
    }

    fn deploy_token_internal(
        &mut self,
        chain_kind: ChainKind,
        token_address: &OmniAddress,
        metadata: BasicMetadata,
        attached_deposit: NearToken,
    ) -> Promise {
        let deployer = self
            .token_deployer_accounts
            .get(&chain_kind)
            .unwrap_or_else(|| env::panic_str("ERR_DEPLOYER_NOT_SET"));
        let prefix = token_address.get_token_prefix();
        let token_id: AccountId = format!("{prefix}.{deployer}")
            .parse()
            .unwrap_or_else(|_| env::panic_str("ERR_PARSE_ACCOUNT"));

        let storage_usage = env::storage_usage();
        self.add_token(
            &token_id,
            token_address,
            metadata.decimals,
            metadata.decimals,
        );

        require!(self.deployed_tokens.insert(&token_id), "ERR_TOKEN_EXIST");
        let required_deposit = env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into())
            .saturating_add(storage::BRIDGE_TOKEN_INIT_BALANCE)
            .saturating_add(NEP141_DEPOSIT);

        require!(
            attached_deposit >= required_deposit,
            "ERROR: The deposit is not sufficient to cover the storage."
        );

        env::log_str(
            &OmniBridgeEvent::DeployTokenEvent {
                token_id: token_id.clone(),
                token_address: token_address.clone(),
                metadata: metadata.clone(),
            }
            .to_log_string(),
        );

        ext_deployer::ext(deployer)
            .with_static_gas(DEPLOY_TOKEN_GAS)
            .with_attached_deposit(storage::BRIDGE_TOKEN_INIT_BALANCE)
            .deploy_token(token_id.clone(), metadata)
            .then(
                ext_token::ext(token_id)
                    .with_static_gas(STORAGE_DEPOSIT_GAS)
                    .with_attached_deposit(NEP141_DEPOSIT)
                    .storage_deposit(&env::current_account_id(), Some(true)),
            )
    }

    fn utxo_fin_transfer(
        &mut self,
        token_id: AccountId,
        amount: U128,
        signer_id: &AccountId,
        sender_id: &AccountId,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
    ) -> PromiseOrPromiseIndexOrValue<U128> {
        let origin_chain = self
            .get_utxo_chain_by_token(&token_id)
            .sdk_expect("ERR_UTXO_CONFIG_MISSING");
        let config = self
            .utxo_chain_connectors
            .get(&origin_chain)
            .sdk_expect("ERR_UTXO_CONFIG_MISSING");
        require!(
            sender_id == &config.connector,
            "ERR_SENDER_IS_NOT_CONNECTOR"
        );

        let fast_transfer = FastTransfer::from_utxo_transfer(
            utxo_fin_transfer_msg.clone(),
            token_id.clone(),
            amount,
            origin_chain,
        );

        if let Some(status) = self.get_fast_transfer_status(&fast_transfer.id()) {
            return self.utxo_fin_transfer_fast(fast_transfer, status, utxo_fin_transfer_msg);
        }

        if let OmniAddress::Near(recipient) = utxo_fin_transfer_msg.recipient.clone() {
            Self::utxo_fin_transfer_to_near(
                recipient,
                token_id,
                amount,
                utxo_fin_transfer_msg,
                origin_chain,
                signer_id,
            )
            .into()
        } else {
            self.utxo_fin_transfer_to_other_chain(
                token_id,
                amount,
                utxo_fin_transfer_msg,
                origin_chain,
                signer_id,
            )
        }
    }

    fn utxo_fin_transfer_fast(
        &mut self,
        fast_transfer: FastTransfer,
        fast_transfer_status: FastTransferStatus,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
    ) -> PromiseOrPromiseIndexOrValue<U128> {
        require!(
            !fast_transfer_status.finalised,
            "ERR_FAST_TRANSFER_ALREADY_FINALISED"
        );

        let amount = if fast_transfer.recipient.get_chain() == ChainKind::Near {
            self.remove_fast_transfer(&fast_transfer.id());
            fast_transfer.amount
        } else {
            self.mark_fast_transfer_as_finalised(&fast_transfer.id());
            // With transfers to other chain the fee will be claimed after finalization on the destination chain
            U128(fast_transfer.amount.0 - fast_transfer.fee.fee.0)
        };

        self.send_tokens(
            fast_transfer.token_id.clone(),
            fast_transfer_status.relayer,
            amount,
            "",
        );

        env::log_str(
            &OmniBridgeEvent::UtxoTransferEvent {
                token_id: fast_transfer.token_id,
                amount,
                utxo_transfer_message: utxo_fin_transfer_msg,
                new_transfer_id: None,
            }
            .to_log_string(),
        );

        PromiseOrPromiseIndexOrValue::Value(U128(0))
    }

    fn utxo_fin_transfer_to_near(
        recipient: AccountId,
        token_id: AccountId,
        amount: U128,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
        origin_chain: ChainKind,
        storage_owner: &AccountId,
    ) -> Promise {
        let deposit_action = StorageDepositAction {
            account_id: recipient.clone(),
            token_id: token_id.clone(),
            storage_deposit_amount: None,
        };

        Self::check_or_pay_ft_storage(&deposit_action, &mut NearToken::from_yoctonear(0)).then(
            Self::ext(env::current_account_id())
                .with_static_gas(
                    UTXO_FIN_TRANSFER_CALLBACK_GAS.saturating_add(FT_TRANSFER_CALL_GAS),
                )
                .utxo_fin_transfer_to_near_callback(
                    token_id,
                    recipient,
                    amount,
                    utxo_fin_transfer_msg,
                    origin_chain,
                    storage_owner,
                ),
        )
    }

    fn utxo_fin_transfer_to_near_callback_internal(
        &mut self,
        token_id: AccountId,
        recipient: AccountId,
        amount: U128,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
        origin_chain: ChainKind,
        storage_owner: &AccountId,
    ) -> Promise {
        require!(
            Self::check_storage_balance_result(0),
            "STORAGE_ERR: The transfer recipient is omitted"
        );

        self.send_tokens(
            token_id.clone(),
            recipient,
            amount,
            &utxo_fin_transfer_msg.msg,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(RESOLVE_UTXO_FIN_TRANSFER_GAS)
                .resolve_utxo_fin_transfer(
                    token_id,
                    amount,
                    utxo_fin_transfer_msg,
                    origin_chain,
                    storage_owner,
                ),
        )
    }

    fn utxo_fin_transfer_to_other_chain(
        &mut self,
        token_id: AccountId,
        amount: U128,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
        origin_chain: ChainKind,
        storage_owner: &AccountId,
    ) -> PromiseOrPromiseIndexOrValue<U128> {
        let origin_transfer_id = UnifiedTransferId {
            origin_chain,
            kind: TransferIdKind::Utxo(utxo_fin_transfer_msg.utxo_id.clone()),
        };

        let mut required_storage_balance = self.add_fin_utxo_transfer(&origin_transfer_id);

        self.current_origin_nonce += 1;
        let transfer_message = TransferMessage {
            origin_nonce: self.current_origin_nonce,
            token: OmniAddress::Near(token_id.clone()),
            amount,
            recipient: utxo_fin_transfer_msg.recipient.clone(),
            fee: Fee {
                fee: utxo_fin_transfer_msg.relayer_fee,
                native_fee: U128(0),
            },
            sender: OmniAddress::Near(env::predecessor_account_id()),
            msg: utxo_fin_transfer_msg.msg.clone(),
            destination_nonce: self
                .get_next_destination_nonce(utxo_fin_transfer_msg.recipient.get_chain()),
            origin_transfer_id: Some(origin_transfer_id),
        };

        required_storage_balance = required_storage_balance.saturating_add(
            self.add_transfer_message(transfer_message.clone(), storage_owner.clone()),
        );

        self.update_storage_balance(
            storage_owner.clone(),
            required_storage_balance,
            NearToken::from_yoctonear(0),
        );

        env::log_str(
            &OmniBridgeEvent::UtxoTransferEvent {
                token_id,
                amount,
                utxo_transfer_message: utxo_fin_transfer_msg,
                new_transfer_id: Some(transfer_message.get_transfer_id()),
            }
            .to_log_string(),
        );

        PromiseOrPromiseIndexOrValue::Value(U128(0))
    }

    fn resolve_utxo_fin_transfer_internal(
        &mut self,
        token_id: AccountId,
        amount: U128,
        utxo_fin_transfer_msg: UtxoFinTransferMsg,
        origin_chain: ChainKind,
        storage_owner: &AccountId,
    ) -> U128 {
        let is_ft_transfer_call = !utxo_fin_transfer_msg.msg.is_empty();
        if Self::is_refund_required(is_ft_transfer_call) {
            self.remove_fin_utxo_transfer(
                &UnifiedTransferId {
                    origin_chain,
                    kind: TransferIdKind::Utxo(utxo_fin_transfer_msg.utxo_id.clone()),
                },
                storage_owner,
            );
            amount
        } else {
            env::log_str(
                &OmniBridgeEvent::UtxoTransferEvent {
                    token_id,
                    amount,
                    utxo_transfer_message: utxo_fin_transfer_msg,
                    new_transfer_id: None,
                }
                .to_log_string(),
            );

            U128(0)
        }
    }

    fn send_fee_internal(
        &mut self,
        message: &TransferMessage,
        fee_recipient: AccountId,
        token_fee: u128,
    ) -> PromiseOrValue<()> {
        if message.fee.native_fee.0 != 0 {
            let origin_chain = message.origin_transfer_id.as_ref().map_or_else(
                || message.get_origin_chain(),
                |origin_transfer_id| origin_transfer_id.origin_chain,
            );

            if origin_chain.is_utxo_chain() {
                env::panic_str("Can't have native fee for transfers from UTXO chains")
            } else if origin_chain == ChainKind::Near {
                Promise::new(fee_recipient.clone())
                    .transfer(NearToken::from_yoctonear(message.fee.native_fee.0));
            } else {
                ext_token::ext(self.get_native_token_id(origin_chain))
                    .with_static_gas(MINT_TOKEN_GAS)
                    .mint(fee_recipient.clone(), message.fee.native_fee, None);
            }
        }

        let token = self.get_token_id(&message.token);
        env::log_str(
            &OmniBridgeEvent::ClaimFeeEvent {
                transfer_message: message.clone(),
            }
            .to_log_string(),
        );

        if token_fee > 0 {
            if self.deployed_tokens.contains(&token) {
                PromiseOrValue::Promise(ext_token::ext(token).with_static_gas(MINT_TOKEN_GAS).mint(
                    fee_recipient,
                    U128(token_fee),
                    None,
                ))
            } else {
                PromiseOrValue::Promise(
                    ext_token::ext(token)
                        .with_static_gas(FT_TRANSFER_GAS)
                        .with_attached_deposit(ONE_YOCTO)
                        .ft_transfer(fee_recipient, U128(token_fee), None),
                )
            }
        } else {
            PromiseOrValue::Value(())
        }
    }

    fn add_token(
        &mut self,
        token_id: &AccountId,
        token_address: &OmniAddress,
        decimals: u8,
        origin_decimals: u8,
    ) {
        let chain_kind = token_address.get_chain();
        require!(
            self.token_id_to_address
                .insert(&(chain_kind, token_id.clone()), token_address)
                .is_none(),
            "ERR_TOKEN_EXIST"
        );
        require!(
            self.token_address_to_id
                .insert(token_address, token_id)
                .is_none(),
            "ERR_TOKEN_EXIST"
        );
        require!(
            self.token_decimals
                .insert(
                    token_address,
                    &Decimals {
                        decimals,
                        origin_decimals,
                    }
                )
                .is_none(),
            "ERR_TOKEN_EXIST"
        );
    }
    fn verify_proof(&self, chain_kind: ChainKind, prover_args: Vec<u8>) -> Promise {
        let prover_account_id = self
            .provers
            .get(&chain_kind)
            .unwrap_or_else(|| env::panic_str("ERR_PROVER_FOR_CHAIN_KIND_NOT_REGISTERED"));

        ext_omni_prover_proxy::ext(prover_account_id)
            .with_static_gas(VERIFY_PROOF_GAS)
            .with_attached_deposit(NearToken::from_near(0))
            .verify_proof(prover_args)
    }

    fn refund(account_id: AccountId, amount: NearToken) {
        if !amount.is_zero() {
            Promise::new(account_id).transfer(amount);
        }
    }

    fn denormalize_amount(amount: u128, decimals: Decimals) -> u128 {
        let diff_decimals: u32 = (decimals.origin_decimals - decimals.decimals).into();
        amount * (10_u128.pow(diff_decimals))
    }

    fn normalize_amount(amount: u128, decimals: Decimals) -> u128 {
        let diff_decimals: u32 = (decimals.origin_decimals - decimals.decimals).into();
        amount / (10_u128.pow(diff_decimals))
    }

    // Native tokens always have the same decimals on Near as on origin chain
    fn denormalize_fee(fee: &Fee, decimals: Decimals) -> Fee {
        Fee {
            fee: U128(Self::denormalize_amount(fee.fee.0, decimals)),
            native_fee: fee.native_fee,
        }
    }
}
