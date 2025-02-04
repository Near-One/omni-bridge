#![allow(clippy::too_many_arguments)]
use errors::SdkExpect;
use near_plugins::{
    access_control, access_control_any, pause, AccessControlRole, AccessControllable, Pausable,
    Upgradable,
};

use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, LookupSet};
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near, require, serde_json, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, Promise, PromiseError, PromiseOrValue, PromiseResult,
};
use omni_types::locker_args::{
    BindTokenArgs, ClaimFeeArgs, DeployTokenArgs, FinTransferArgs, StorageDepositAction,
};
use omni_types::mpc_types::SignatureResponse;
use omni_types::near_events::OmniBridgeEvent;
use omni_types::prover_args::VerifyProofArgs;
use omni_types::prover_result::ProverResult;
use omni_types::{
    BasicMetadata, ChainKind, Fee, InitTransferMsg, MetadataPayload, Nonce, OmniAddress,
    PayloadType, SignRequest, TransferId, TransferMessage, TransferMessagePayload, UpdateFee, H160,
};
use std::str::FromStr;
use storage::{Decimals, TransferMessageStorage, TransferMessageStorageValue, NEP141_DEPOSIT};

mod errors;
mod storage;

#[cfg(test)]
mod tests;

const LOG_METADATA_GAS: Gas = Gas::from_tgas(10);
const LOG_METADATA_CALLBACK_GAS: Gas = Gas::from_tgas(260);
const MPC_SIGNING_GAS: Gas = Gas::from_tgas(250);
const SIGN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const SIGN_LOG_METADATA_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const VERIFY_PROOF_GAS: Gas = Gas::from_tgas(30);
const VERIFY_PROOF_CALLBACK_GAS: Gas = Gas::from_tgas(150);
const CLAIM_FEE_CALLBACK_GAS: Gas = Gas::from_tgas(50);
const BIND_TOKEN_CALLBACK_GAS: Gas = Gas::from_tgas(25);
const BIND_TOKEN_REFUND_GAS: Gas = Gas::from_tgas(5);
const FT_TRANSFER_CALL_GAS: Gas = Gas::from_tgas(125);
const FT_TRANSFER_GAS: Gas = Gas::from_tgas(5);
const UPDATE_CONTROLLER_GAS: Gas = Gas::from_tgas(250);
const WNEAR_WITHDRAW_GAS: Gas = Gas::from_tgas(10);
const STORAGE_BALANCE_OF_GAS: Gas = Gas::from_tgas(3);
const STORAGE_DEPOSIT_GAS: Gas = Gas::from_tgas(3);
const DEPLOY_TOKEN_CALLBACK_GAS: Gas = Gas::from_tgas(75);
const DEPLOY_TOKEN_GAS: Gas = Gas::from_tgas(50);
const BURN_TOKEN_GAS: Gas = Gas::from_tgas(10);
const MINT_TOKEN_GAS: Gas = Gas::from_tgas(5);
const SET_METADATA_GAS: Gas = Gas::from_tgas(10);
const NO_DEPOSIT: NearToken = NearToken::from_near(0);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const SIGN_PATH: &str = "bridge-1";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    PendingTransfers,
    Factories,
    FinalisedTransfers,
    TokenIdToAddress,
    AccountsBalances,
    TokenAddressToId,
    TokenDeployerAccounts,
    DeployedTokens,
    DestinationNonces,
    TokenDecimals,
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

#[ext_contract(ext_bridge_token_facory)]
pub trait ExtBridgeTokenFactory {
    fn set_controller_for_tokens(&self, tokens_account_id: Vec<AccountId>);
}

#[ext_contract(ext_signer)]
pub trait ExtSigner {
    fn sign(&mut self, request: SignRequest);
}

#[ext_contract(ext_prover)]
pub trait Prover {
    #[result_serializer(borsh)]
    fn verify_proof(&self, #[serializer(borsh)] args: VerifyProofArgs) -> ProverResult;
}

#[ext_contract(ext_wnear_token)]
pub trait ExtWNearToken {
    fn near_withdraw(&self, amount: U128);
}

#[ext_contract(ext_deployer)]
pub trait TokenDeployer {
    fn deploy_token(&self, account_id: AccountId, metadata: BasicMetadata) -> Promise;
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
    pub pending_transfers: LookupMap<TransferId, TransferMessageStorage>,
    pub finalised_transfers: LookupSet<TransferId>,
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
}

#[near]
impl FungibleTokenReceiver for Contract {
    #[pause(except(roles(Role::DAO, Role::UnrestrictedDeposit)))]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let parsed_msg: InitTransferMsg = serde_json::from_str(&msg).sdk_expect("ERR_PARSE_MSG");
        let token_id = env::predecessor_account_id();
        require!(
            parsed_msg.recipient.get_chain() != ChainKind::Near,
            "ERR_INVALID_RECIPIENT_CHAIN"
        );

        self.current_origin_nonce += 1;
        let destination_nonce = self.get_next_destination_nonce(parsed_msg.recipient.get_chain());

        let transfer_message = TransferMessage {
            origin_nonce: self.current_origin_nonce,
            token: OmniAddress::Near(token_id.clone()),
            amount,
            recipient: parsed_msg.recipient,
            fee: Fee {
                fee: parsed_msg.fee,
                native_fee: parsed_msg.native_token_fee,
            },
            sender: OmniAddress::Near(sender_id.clone()),
            msg: String::new(),
            destination_nonce,
        };
        require!(
            transfer_message.fee.fee < transfer_message.amount,
            "ERR_INVALID_FEE"
        );

        let mut required_storage_balance =
            self.add_transfer_message(transfer_message.clone(), sender_id.clone());
        required_storage_balance = required_storage_balance
            .saturating_add(NearToken::from_yoctonear(parsed_msg.native_token_fee.0));

        self.update_storage_balance(
            sender_id,
            required_storage_balance,
            NearToken::from_yoctonear(0),
        );

        if self.deployed_tokens.contains(&token_id) {
            ext_token::ext(token_id.clone())
                .with_static_gas(BURN_TOKEN_GAS)
                .burn(amount);
        }

        env::log_str(&OmniBridgeEvent::InitTransferEvent { transfer_message }.to_log_string());
        PromiseOrValue::Value(U128(0))
    }
}

#[near]
impl Contract {
    #[init]
    pub fn new(
        prover_account: AccountId,
        mpc_signer: AccountId,
        wnear_account_id: AccountId,
    ) -> Self {
        let mut contract = Self {
            prover_account,
            factories: LookupMap::new(StorageKey::Factories),
            pending_transfers: LookupMap::new(StorageKey::PendingTransfers),
            finalised_transfers: LookupSet::new(StorageKey::FinalisedTransfers),
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

                require!(
                    OmniAddress::Near(env::predecessor_account_id()) == transfer.message.sender,
                    "Only sender can update fee"
                );

                let current_fee = transfer.message.fee;
                require!(
                    fee.fee >= current_fee.fee && fee.fee < transfer.message.amount,
                    "ERR_INVALID_FEE"
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

        let payload = near_sdk::env::keccak256_array(&borsh::to_vec(&transfer_payload).unwrap());

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
        let main_promise = ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_PROOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(VerifyProofArgs {
                prover_id: args.chain_kind.as_ref().to_owned(),
                prover_args: args.prover_args,
            });

        let mut attached_deposit = env::attached_deposit();
        Self::check_or_pay_ft_storage(
            main_promise,
            &args.storage_deposit_actions,
            &mut attached_deposit,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_attached_deposit(attached_deposit)
                .with_static_gas(VERIFY_PROOF_CALLBACK_GAS)
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
            fee: Fee {
                fee: Self::denormalize_amount(init_transfer.fee.fee.0, decimals).into(),
                native_fee: init_transfer.fee.native_fee,
            },
            sender: init_transfer.sender,
            msg: init_transfer.msg,
            destination_nonce,
        };

        if let OmniAddress::Near(recipient) = transfer_message.recipient.clone() {
            self.process_fin_transfer_to_near(
                recipient,
                predecessor_account_id,
                transfer_message,
                storage_deposit_actions,
            )
            .into()
        } else {
            self.process_fin_transfer_to_other_cahin(predecessor_account_id, transfer_message);
            PromiseOrValue::Value(destination_nonce)
        }
    }

    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn claim_fee(&mut self, #[serializer(borsh)] args: ClaimFeeArgs) -> Promise {
        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_PROOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(VerifyProofArgs {
                prover_id: args.chain_kind.as_ref().to_owned(),
                prover_args: args.prover_args,
            })
            .then(
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
        require!(
            fin_transfer.fee_recipient == *predecessor_account_id,
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
        let token_address = self
            .get_token_address(
                message.get_destination_chain(),
                self.get_token_id(&message.token),
            )
            .unwrap_or_else(|| env::panic_str("ERR_FAILED_TO_GET_TOKEN_ADDRESS"));

        let denormalized_amount = Self::denormalize_amount(
            fin_transfer.amount.0,
            self.token_decimals
                .get(&token_address)
                .sdk_expect("ERR_TOKEN_DECIMALS_NOT_FOUND"),
        );
        let fee = message.amount.0 - denormalized_amount;

        if message.fee.native_fee.0 != 0 {
            if message.get_origin_chain() == ChainKind::Near {
                Promise::new(fin_transfer.fee_recipient.clone())
                    .transfer(NearToken::from_yoctonear(message.fee.native_fee.0));
            } else {
                ext_token::ext(self.get_native_token_id(message.get_origin_chain()))
                    .with_static_gas(MINT_TOKEN_GAS)
                    .mint(
                        fin_transfer.fee_recipient.clone(),
                        message.fee.native_fee,
                        None,
                    );
            }
        }

        let token = self.get_token_id(&message.token);
        env::log_str(
            &OmniBridgeEvent::ClaimFeeEvent {
                transfer_message: message,
            }
            .to_log_string(),
        );

        if fee > 0 {
            if self.deployed_tokens.contains(&token) {
                PromiseOrValue::Promise(ext_token::ext(token).with_static_gas(MINT_TOKEN_GAS).mint(
                    fin_transfer.fee_recipient,
                    U128(fee),
                    None,
                ))
            } else {
                PromiseOrValue::Promise(
                    ext_token::ext(token)
                        .with_static_gas(FT_TRANSFER_GAS)
                        .with_attached_deposit(ONE_YOCTO)
                        .ft_transfer(fin_transfer.fee_recipient, U128(fee), None),
                )
            }
        } else {
            PromiseOrValue::Value(())
        }
    }

    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn deploy_token(&mut self, #[serializer(borsh)] args: DeployTokenArgs) -> Promise {
        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_PROOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(VerifyProofArgs {
                prover_id: args.chain_kind.as_ref().to_owned(),
                prover_args: args.prover_args,
            })
            .then(
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
        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_PROOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(VerifyProofArgs {
                prover_id: args.chain_kind.as_ref().to_owned(),
                prover_args: args.prover_args,
            })
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
        self.token_id_to_address.insert(
            &(
                deploy_token.token_address.get_chain(),
                deploy_token.token.clone(),
            ),
            &deploy_token.token_address,
        );
        self.token_address_to_id
            .insert(&deploy_token.token_address, &deploy_token.token);

        self.token_decimals.insert(
            &deploy_token.token_address,
            &Decimals {
                decimals: deploy_token.decimals,
                origin_decimals: deploy_token.origin_decimals,
            },
        );

        let required_deposit = env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into());

        require!(
            attached_deposit >= required_deposit,
            "ERROR: The deposit is not sufficient to cover the storage."
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
        let refund_amount = call_result.unwrap_or(env::attached_deposit());
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
            token: OmniAddress::Near(token_id.clone()),
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

    pub fn is_transfer_finalised(&self, transfer_id: TransferId) -> bool {
        self.finalised_transfers.contains(&transfer_id)
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
    #[payable]
    pub fn add_deployed_tokens(&mut self, tokens: Vec<(OmniAddress, AccountId)>) {
        require!(
            env::attached_deposit() >= NEP141_DEPOSIT.saturating_mul(tokens.len() as u128),
            "ERR_NOT_ENOUGH_ATTACHED_DEPOSIT"
        );

        for (token_address, token_id) in tokens {
            self.deployed_tokens.insert(&token_id);
            self.token_address_to_id.insert(&token_address, &token_id);
            self.token_id_to_address.insert(
                &(token_address.get_chain(), token_id.clone()),
                &token_address,
            );
            self.token_decimals.insert(
                &token_address,
                &Decimals {
                    decimals: 0,
                    origin_decimals: 0,
                },
            );

            ext_token::ext(token_id)
                .with_static_gas(STORAGE_DEPOSIT_GAS)
                .with_attached_deposit(NEP141_DEPOSIT)
                .storage_deposit(&env::current_account_id(), Some(true));
        }
    }

    #[access_control_any(roles(Role::DAO, Role::MetadataManager))]
    pub fn set_token_metadata(
        &mut self,
        token: AccountId,
        name: Option<String>,
        symbol: Option<String>,
        icon: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
    ) -> Promise {
        ext_token::ext(token)
            .with_static_gas(SET_METADATA_GAS)
            .set_metadata(name, symbol, reference, reference_hash, None, icon)
    }

    pub fn get_current_destination_nonce(&self, chain_kind: ChainKind) -> Nonce {
        self.destination_nonces.get(&chain_kind).unwrap_or_default()
    }

    pub fn get_mpc_account(&self) -> AccountId {
        self.mpc_signer.clone()
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
}

impl Contract {
    fn get_next_destination_nonce(&mut self, chain_kind: ChainKind) -> Nonce {
        if chain_kind == ChainKind::Near {
            return 0;
        }

        let mut payload_nonce = self.destination_nonces.get(&chain_kind).unwrap_or_default();

        payload_nonce += 1;

        self.destination_nonces.insert(&chain_kind, &payload_nonce);

        payload_nonce
    }

    #[allow(clippy::too_many_lines, clippy::ptr_arg)]
    fn process_fin_transfer_to_near(
        &mut self,
        recipient: AccountId,
        predecessor_account_id: AccountId,
        transfer_message: TransferMessage,
        storage_deposit_actions: &Vec<StorageDepositAction>,
    ) -> Promise {
        let mut required_balance = self.add_fin_transfer(&transfer_message.get_transfer_id());

        let token = self.get_token_id(&transfer_message.token);

        let mut storage_deposit_action_index: usize = 0;
        require!(
            Self::check_storage_balance_result((storage_deposit_action_index + 1) as u64)
                && storage_deposit_actions[storage_deposit_action_index].account_id == recipient
                && storage_deposit_actions[storage_deposit_action_index].token_id == token,
            "STORAGE_ERR: The transfer recipient is omitted"
        );
        storage_deposit_action_index += 1;

        let amount_to_transfer = U128(transfer_message.amount.0 - transfer_message.fee.fee.0);
        let is_deployed_token = self.deployed_tokens.contains(&token);

        let mut promise = if token == self.wnear_account_id && transfer_message.msg.is_empty() {
            // Unwrap wNEAR and transfer NEAR tokens
            ext_wnear_token::ext(self.wnear_account_id.clone())
                .with_static_gas(WNEAR_WITHDRAW_GAS)
                .with_attached_deposit(ONE_YOCTO)
                .near_withdraw(amount_to_transfer)
                .then(
                    Promise::new(recipient)
                        .transfer(NearToken::from_yoctonear(amount_to_transfer.0)),
                )
        } else if is_deployed_token {
            let deposit = if transfer_message.msg.is_empty() {
                NO_DEPOSIT
            } else {
                ONE_YOCTO
            };
            ext_token::ext(token.clone())
                .with_attached_deposit(deposit)
                .with_static_gas(MINT_TOKEN_GAS.saturating_add(FT_TRANSFER_CALL_GAS))
                .mint(
                    recipient,
                    amount_to_transfer,
                    (!transfer_message.msg.is_empty()).then(|| transfer_message.msg.clone()),
                )
        } else if transfer_message.msg.is_empty() {
            ext_token::ext(token.clone())
                .with_attached_deposit(ONE_YOCTO)
                .with_static_gas(FT_TRANSFER_GAS)
                .ft_transfer(recipient, amount_to_transfer, None)
        } else {
            ext_token::ext(token.clone())
                .with_attached_deposit(ONE_YOCTO)
                .with_static_gas(FT_TRANSFER_CALL_GAS)
                .ft_transfer_call(
                    recipient,
                    amount_to_transfer,
                    None,
                    transfer_message.msg.clone(),
                )
        };

        if transfer_message.fee.fee.0 > 0 {
            require!(
                Self::check_storage_balance_result((storage_deposit_action_index + 1) as u64)
                    && storage_deposit_actions[storage_deposit_action_index].account_id
                        == predecessor_account_id
                    && storage_deposit_actions[storage_deposit_action_index].token_id == token,
                "STORAGE_ERR: The fee recipient is omitted"
            );
            storage_deposit_action_index += 1;

            promise = promise.then(if is_deployed_token {
                ext_token::ext(token).with_static_gas(MINT_TOKEN_GAS).mint(
                    predecessor_account_id.clone(),
                    transfer_message.fee.fee,
                    None,
                )
            } else {
                ext_token::ext(token)
                    .with_attached_deposit(ONE_YOCTO)
                    .with_static_gas(FT_TRANSFER_GAS)
                    .ft_transfer(
                        predecessor_account_id.clone(),
                        transfer_message.fee.fee,
                        None,
                    )
            });

            required_balance = required_balance.saturating_add(NearToken::from_yoctonear(2));
        } else {
            required_balance = required_balance.saturating_add(ONE_YOCTO);
        }

        if transfer_message.fee.native_fee.0 > 0 {
            let native_token_id = self.get_native_token_id(transfer_message.get_origin_chain());

            require!(
                Self::check_storage_balance_result((storage_deposit_action_index + 1) as u64)
                    && storage_deposit_actions[storage_deposit_action_index].account_id
                        == predecessor_account_id
                    && storage_deposit_actions[storage_deposit_action_index].token_id
                        == native_token_id,
                "STORAGE_ERR: The native fee recipient is omitted"
            );

            promise = promise.then(
                ext_token::ext(native_token_id)
                    .with_static_gas(MINT_TOKEN_GAS)
                    .mint(
                        predecessor_account_id.clone(),
                        transfer_message.fee.native_fee,
                        None,
                    ),
            );
        }

        self.update_storage_balance(
            predecessor_account_id,
            required_balance,
            env::attached_deposit(),
        );

        env::log_str(&OmniBridgeEvent::FinTransferEvent { transfer_message }.to_log_string());

        promise
    }

    fn process_fin_transfer_to_other_cahin(
        &mut self,
        predecessor_account_id: AccountId,
        transfer_message: TransferMessage,
    ) {
        let mut required_balance = self.add_fin_transfer(&transfer_message.get_transfer_id());

        required_balance = self
            .add_transfer_message(transfer_message.clone(), predecessor_account_id.clone())
            .saturating_add(required_balance);

        self.update_storage_balance(
            predecessor_account_id,
            required_balance,
            env::attached_deposit(),
        );

        env::log_str(&OmniBridgeEvent::FinTransferEvent { transfer_message }.to_log_string());
    }

    fn check_or_pay_ft_storage(
        mut main_promise: Promise,
        storage_deposit_actions: &Vec<StorageDepositAction>,
        attached_deposit: &mut NearToken,
    ) -> Promise {
        for action in storage_deposit_actions {
            let promise = if let Some(storage_deposit_amount) = action.storage_deposit_amount {
                let storage_deposit_amount = NearToken::from_yoctonear(storage_deposit_amount);

                *attached_deposit = attached_deposit
                    .checked_sub(storage_deposit_amount)
                    .sdk_expect("The attached deposit is less than required");

                ext_token::ext(action.token_id.clone())
                    .with_static_gas(STORAGE_DEPOSIT_GAS)
                    .with_attached_deposit(storage_deposit_amount)
                    .storage_deposit(&action.account_id, Some(true))
            } else {
                ext_token::ext(action.token_id.clone())
                    .with_static_gas(STORAGE_BALANCE_OF_GAS)
                    .with_attached_deposit(NO_DEPOSIT)
                    .storage_balance_of(&action.account_id)
            };

            main_promise = main_promise.and(promise);
        }

        main_promise
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

    fn update_storage_balance(
        &mut self,
        account_id: AccountId,
        required_balance: NearToken,
        attached_deposit: NearToken,
    ) {
        if attached_deposit >= required_balance {
            Self::refund(
                account_id,
                attached_deposit.saturating_sub(required_balance),
            );
        } else {
            let required_balance = required_balance.saturating_sub(attached_deposit);
            let mut storage_balance = self
                .accounts_balances
                .get(&account_id)
                .sdk_expect("ERR_ACCOUNT_NOT_REGISTERED");

            if storage_balance.available >= required_balance {
                storage_balance.available =
                    storage_balance.available.saturating_sub(required_balance);
                self.accounts_balances.insert(&account_id, &storage_balance);
            } else {
                env::panic_str("Not enough storage deposited");
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
        require!(
            self.token_id_to_address
                .insert(&(chain_kind, token_id.clone()), token_address)
                .is_none(),
            "ERR_TOKEN_EXIST"
        );
        require!(
            self.token_address_to_id
                .insert(token_address, &token_id)
                .is_none(),
            "ERR_TOKEN_EXIST"
        );
        require!(
            self.token_decimals
                .insert(
                    token_address,
                    &Decimals {
                        decimals: metadata.decimals,
                        origin_decimals: metadata.decimals
                    }
                )
                .is_none(),
            "ERR_TOKEN_EXIST"
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
}
