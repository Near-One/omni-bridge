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
    BindTokenArgs, ClaimFeeArgs, DeployTokenArgs, FinTransferArgs, StorageDepositArgs,
};
use omni_types::mpc_types::SignatureResponse;
use omni_types::near_events::Nep141LockerEvent;
use omni_types::prover_args::VerifyProofArgs;
use omni_types::prover_result::ProverResult;
use omni_types::{
    BasicMetadata, ChainKind, ClaimNativeFeePayload, Fee, InitTransferMsg, MetadataPayload,
    NativeFee, Nonce, OmniAddress, PayloadType, SignRequest, TransferId, TransferMessage,
    TransferMessagePayload, UpdateFee,
};
use storage::{TransferMessageStorage, TransferMessageStorageValue};

mod errors;
mod storage;

#[cfg(test)]
mod tests;

const LOG_METADATA_GAS: Gas = Gas::from_tgas(10);
const LOG_METADATA_CALLBACK_GAS: Gas = Gas::from_tgas(260);
const MPC_SIGNING_GAS: Gas = Gas::from_tgas(250);
const SIGN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const SIGN_LOG_METADATA_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const SIGN_CLAIM_NATIVE_FEE_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const VERIFY_PROOF_GAS: Gas = Gas::from_tgas(50);
const CLAIM_FEE_CALLBACK_GAS: Gas = Gas::from_tgas(50);
const BIND_TOKEN_CALLBACK_GAS: Gas = Gas::from_tgas(25);
const BIND_TOKEN_REFUND_GAS: Gas = Gas::from_tgas(5);
const FT_TRANSFER_CALL_GAS: Gas = Gas::from_tgas(50);
const FT_TRANSFER_GAS: Gas = Gas::from_tgas(5);
const WNEAR_WITHDRAW_GAS: Gas = Gas::from_tgas(10);
const STORAGE_BALANCE_OF_GAS: Gas = Gas::from_tgas(3);
const STORAGE_DEPOSIT_GAS: Gas = Gas::from_tgas(3);
const DEPLOY_TOKEN_CALLBACK_GAS: Gas = Gas::from_tgas(75);
const DEPLOY_TOKEN_GAS: Gas = Gas::from_tgas(50);
const BURN_TOKEN_GAS: Gas = Gas::from_tgas(10);
const MINT_TOKEN_GAS: Gas = Gas::from_tgas(10);
const SET_METADATA_GAS: Gas = Gas::from_tgas(10);
const NO_DEPOSIT: NearToken = NearToken::from_near(0);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const NEP141_DEPOSIT: NearToken = NearToken::from_yoctonear(1_250_000_000_000_000_000_000);
const BRIDGE_TOKEN_INIT_BALANCE: NearToken = NearToken::from_near(3);
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
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> Option<StorageBalance>;

    fn storage_balance_of(&mut self, account_id: Option<AccountId>) -> Option<StorageBalance>;

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
    pub pending_transfers: LookupMap<Nonce, TransferMessageStorage>,
    pub finalised_transfers: LookupMap<TransferId, Option<NativeFee>>,
    pub token_id_to_address: LookupMap<(ChainKind, AccountId), OmniAddress>,
    pub token_address_to_id: LookupMap<OmniAddress, AccountId>,
    pub deployed_tokens: LookupSet<AccountId>,
    pub token_deployer_accounts: LookupMap<ChainKind, AccountId>,
    pub mpc_signer: AccountId,
    pub current_nonce: Nonce,
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

        self.current_nonce += 1;
        let transfer_message = TransferMessage {
            origin_nonce: U128(self.current_nonce),
            token: OmniAddress::Near(token_id.clone()),
            amount,
            recipient: parsed_msg.recipient,
            fee: Fee {
                fee: parsed_msg.fee,
                native_fee: parsed_msg.native_token_fee,
            },
            sender: OmniAddress::Near(sender_id.clone()),
            msg: String::new(),
        };
        require!(
            transfer_message.fee.fee < transfer_message.amount,
            "ERR_INVALID_FEE"
        );

        let mut required_storage_balance = self.add_transfer_message(
            self.current_nonce,
            transfer_message.clone(),
            sender_id.clone(),
        );
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

        env::log_str(&Nep141LockerEvent::InitTransferEvent { transfer_message }.to_log_string());
        PromiseOrValue::Value(U128(0))
    }
}

#[near]
impl Contract {
    #[init]
    pub fn new(
        prover_account: AccountId,
        mpc_signer: AccountId,
        nonce: U128,
        wnear_account_id: AccountId,
    ) -> Self {
        let mut contract = Self {
            prover_account,
            factories: LookupMap::new(StorageKey::Factories),
            pending_transfers: LookupMap::new(StorageKey::PendingTransfers),
            finalised_transfers: LookupMap::new(StorageKey::FinalisedTransfers),
            token_id_to_address: LookupMap::new(StorageKey::TokenIdToAddress),
            token_address_to_id: LookupMap::new(StorageKey::TokenAddressToId),
            deployed_tokens: LookupSet::new(StorageKey::DeployedTokens),
            token_deployer_accounts: LookupMap::new(StorageKey::TokenDeployerAccounts),
            mpc_signer,
            current_nonce: nonce.0,
            accounts_balances: LookupMap::new(StorageKey::AccountsBalances),
            wnear_account_id,
        };

        contract.acl_init_super_admin(near_sdk::env::predecessor_account_id());
        contract.acl_grant_role("DAO".to_owned(), near_sdk::env::predecessor_account_id());
        contract
    }

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
                &Nep141LockerEvent::LogMetadataEvent {
                    signature,
                    metadata_payload,
                }
                .to_log_string(),
            );
        }
    }

    #[payable]
    pub fn update_transfer_fee(&mut self, nonce: U128, fee: UpdateFee) {
        match fee {
            UpdateFee::Fee(fee) => {
                let mut transfer = self.get_transfer_message_storage(nonce);

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
                self.insert_raw_transfer(nonce.0, transfer.message.clone(), transfer.owner);

                env::log_str(
                    &Nep141LockerEvent::UpdateFeeEvent {
                        transfer_message: transfer.message,
                    }
                    .to_log_string(),
                );
            }
            UpdateFee::Proof(_) => env::panic_str("TODO"),
        }
    }

    #[payable]
    pub fn sign_claim_native_fee(&mut self, nonces: Vec<U128>, recipient: OmniAddress) -> Promise {
        let chain_kind = recipient.get_chain();
        let mut amount: u128 = 0_u128;
        for nonce in &nonces {
            let native_fee = self
                .finalised_transfers
                .get(&(chain_kind, nonce.0))
                .flatten()
                .sdk_expect("ERR_NATIVE_FEE_NOT_EXISIT");

            require!(native_fee.recipient == recipient, "ERR_WRONG_RECIPIENT");
            amount += native_fee.amount.0;
        }

        let claim_payload = ClaimNativeFeePayload {
            prefix: PayloadType::ClaimNativeFee,
            nonces,
            amount: U128(amount),
            recipient,
        };
        let payload =
            near_sdk::env::keccak256_array(&borsh::to_vec(&claim_payload).sdk_expect("ERR_BORSH"));

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
                    .with_static_gas(SIGN_CLAIM_NATIVE_FEE_CALLBACK_GAS)
                    .sign_claim_native_fee_callback(claim_payload),
            )
    }

    #[private]
    pub fn sign_claim_native_fee_callback(
        &mut self,
        #[callback_result] call_result: Result<SignatureResponse, PromiseError>,
        #[serializer(borsh)] claim_payload: ClaimNativeFeePayload,
    ) {
        if let Ok(signature) = call_result {
            env::log_str(
                &Nep141LockerEvent::SignClaimNativeFeeEvent {
                    signature,
                    claim_payload,
                }
                .to_log_string(),
            );
        }
    }

    /// # Panics
    ///
    /// This function will panic under the following conditions:
    ///
    /// - If the `borsh::to_vec` serialization of the `TransferMessagePayload` fails.
    /// - If a `fee` is provided and it doesn't match the fee in the stored transfer message.
    #[payable]
    pub fn sign_transfer(
        &mut self,
        nonce: U128,
        fee_recipient: Option<AccountId>,
        fee: &Option<Fee>,
    ) -> Promise {
        let transfer_message = self.get_transfer_message(nonce);
        if let Some(fee) = &fee {
            require!(&transfer_message.fee == fee, "Invalid fee");
        }

        let token_address = self
            .get_token_address(
                transfer_message.get_destination_chain(),
                self.get_token_id(&transfer_message.token),
            )
            .unwrap_or_else(|| env::panic_str("ERR_FAILED_TO_GET_TOKEN_ADDRESS"));

        let transfer_payload = TransferMessagePayload {
            prefix: PayloadType::TransferMessage,
            nonce,
            token_address,
            amount: U128(transfer_message.amount.0 - transfer_message.fee.fee.0),
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
            let nonce = message_payload.nonce;
            if fee.is_zero() {
                self.remove_transfer_message(nonce.0);
            }

            env::log_str(
                &Nep141LockerEvent::SignTransferEvent {
                    signature,
                    message_payload,
                }
                .to_log_string(),
            );
        }
    }

    #[payable]
    pub fn fin_transfer(&mut self, #[serializer(borsh)] args: FinTransferArgs) -> Promise {
        require!(
            args.storage_deposit_args.accounts.len() <= 2,
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
            &args.storage_deposit_args,
            &mut attached_deposit,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_attached_deposit(attached_deposit)
                .with_static_gas(CLAIM_FEE_CALLBACK_GAS)
                .fin_transfer_callback(
                    &args.storage_deposit_args,
                    env::predecessor_account_id(),
                    args.native_fee_recipient,
                ),
        )
    }

    // TODO: try to split this function
    #[allow(clippy::too_many_lines)]
    #[private]
    #[payable]
    pub fn fin_transfer_callback(
        &mut self,
        #[serializer(borsh)] storage_deposit_args: &StorageDepositArgs,
        #[serializer(borsh)] predecessor_account_id: AccountId,
        #[serializer(borsh)] native_fee_recipient: Option<OmniAddress>,
    ) -> PromiseOrValue<U128> {
        let Ok(ProverResult::InitTransfer(init_transfer)) = Self::decode_prover_result(0) else {
            env::panic_str("Invalid proof message")
        };
        require!(
            self.factories
                .get(&init_transfer.emitter_address.get_chain())
                == Some(init_transfer.emitter_address),
            "Unknown factory"
        );

        let transfer_message = init_transfer.transfer;
        let mut required_balance;

        if let OmniAddress::Near(recipient) = &transfer_message.recipient {
            let native_fee = if transfer_message.fee.native_fee.0 != 0 {
                let recipient = native_fee_recipient.sdk_expect("ERR_FEE_RECIPIENT_NOT_SET");
                require!(
                    transfer_message.get_origin_chain() == recipient.get_chain(),
                    "ERR_WRONG_FEE_RECIPIENT_CHAIN"
                );
                Some(NativeFee {
                    amount: transfer_message.fee.native_fee,
                    recipient,
                })
            } else {
                None
            };

            required_balance =
                self.add_fin_transfer(&transfer_message.get_transfer_id(), &native_fee);

            let token = self.get_token_id(&transfer_message.token);
            require!(
                token == storage_deposit_args.token,
                "STORAGE_ERR: Invalid token"
            );
            require!(
                Self::check_storage_balance_result(1)
                    && &storage_deposit_args.accounts[0].0 == recipient,
                "STORAGE_ERR: The transfer recipient is omitted"
            );

            let amount_to_transfer = U128(transfer_message.amount.0 - transfer_message.fee.fee.0);
            let is_deployed_token = self.deployed_tokens.contains(&token);

            let mut promise = if token == self.wnear_account_id && transfer_message.msg.is_empty() {
                ext_wnear_token::ext(self.wnear_account_id.clone())
                    .with_static_gas(WNEAR_WITHDRAW_GAS)
                    .with_attached_deposit(ONE_YOCTO)
                    .near_withdraw(amount_to_transfer)
                    .then(
                        Promise::new(recipient.clone())
                            .transfer(NearToken::from_yoctonear(amount_to_transfer.0)),
                    )
            } else {
                let transfer = ext_token::ext(token.clone()).with_attached_deposit(ONE_YOCTO);
                if is_deployed_token {
                    transfer
                        .with_static_gas(MINT_TOKEN_GAS.saturating_add(FT_TRANSFER_CALL_GAS))
                        .mint(
                            recipient.clone(),
                            amount_to_transfer,
                            (!transfer_message.msg.is_empty())
                                .then(|| transfer_message.msg.clone()),
                        )
                } else if transfer_message.msg.is_empty() {
                    transfer.with_static_gas(FT_TRANSFER_GAS).ft_transfer(
                        recipient.clone(),
                        amount_to_transfer,
                        None,
                    )
                } else {
                    transfer
                        .with_static_gas(FT_TRANSFER_CALL_GAS)
                        .ft_transfer_call(
                            recipient.clone(),
                            amount_to_transfer,
                            None,
                            transfer_message.msg.clone(),
                        )
                }
            };

            if transfer_message.fee.fee.0 > 0 {
                require!(
                    Self::check_storage_balance_result(2)
                        && storage_deposit_args.accounts[1].0 == predecessor_account_id,
                    "STORAGE_ERR: The fee recipient is omitted"
                );

                if is_deployed_token {
                    promise =
                        promise.then(ext_token::ext(token).with_static_gas(MINT_TOKEN_GAS).mint(
                            predecessor_account_id.clone(),
                            transfer_message.fee.fee,
                            None,
                        ));
                } else {
                    promise = promise.then(
                        ext_token::ext(token)
                            .with_static_gas(FT_TRANSFER_GAS)
                            .with_attached_deposit(ONE_YOCTO)
                            .ft_transfer(
                                predecessor_account_id.clone(),
                                transfer_message.fee.fee,
                                None,
                            ),
                    );
                }

                required_balance = required_balance.saturating_add(NearToken::from_yoctonear(2));
            } else {
                required_balance = required_balance.saturating_add(ONE_YOCTO);
            }

            self.update_storage_balance(
                predecessor_account_id,
                required_balance,
                env::attached_deposit(),
            );

            env::log_str(
                &Nep141LockerEvent::FinTransferEvent {
                    nonce: None,
                    transfer_message,
                }
                .to_log_string(),
            );

            promise.into()
        } else {
            required_balance = self.add_fin_transfer(&transfer_message.get_transfer_id(), &None);
            self.current_nonce += 1;
            required_balance = self
                .add_transfer_message(
                    self.current_nonce,
                    transfer_message.clone(),
                    predecessor_account_id.clone(),
                )
                .saturating_add(required_balance);

            self.update_storage_balance(
                predecessor_account_id,
                required_balance,
                env::attached_deposit(),
            );

            env::log_str(
                &Nep141LockerEvent::FinTransferEvent {
                    nonce: Some(U128(self.current_nonce)),
                    transfer_message,
                }
                .to_log_string(),
            );

            PromiseOrValue::Value(U128(self.current_nonce))
        }
    }

    #[payable]
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
                    .claim_fee_callback(args.native_fee_recipient, env::predecessor_account_id()),
            )
    }

    #[private]
    #[payable]
    pub fn claim_fee_callback(
        &mut self,
        #[serializer(borsh)] native_fee_recipient: Option<OmniAddress>,
        #[serializer(borsh)] predecessor_account_id: AccountId,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProverResult, PromiseError>,
    ) -> PromiseOrValue<()> {
        let Ok(ProverResult::FinTransfer(fin_transfer)) = call_result else {
            env::panic_str("Invalid proof message")
        };
        require!(
            fin_transfer.fee_recipient == predecessor_account_id,
            "ERR_ONLY_FEE_RECIPIENT_CAN_CLAIM"
        );
        require!(
            self.factories
                .get(&fin_transfer.emitter_address.get_chain())
                == Some(fin_transfer.emitter_address),
            "ERR_UNKNOWN_FACTORY"
        );

        let message = self.remove_transfer_message(fin_transfer.nonce.0);
        let fee = message.amount.0 - fin_transfer.amount.0;

        if message.fee.native_fee.0 != 0 {
            let native_fee_recipient = native_fee_recipient.sdk_expect("ERR_FEE_RECIPIENT_NOT_SET");
            require!(
                message.get_origin_chain() == native_fee_recipient.get_chain(),
                "ERR_WRONG_FEE_RECIPIENT_CHAIN"
            );

            if message.get_origin_chain() == ChainKind::Near {
                let OmniAddress::Near(near_recipient) = &native_fee_recipient else {
                    env::panic_str("ERR_WRONG_CHAIN_KIND")
                };
                Promise::new(near_recipient.clone())
                    .transfer(NearToken::from_yoctonear(message.fee.native_fee.0));
            } else {
                let required_balance = self.update_fin_transfer(
                    &message.get_transfer_id(),
                    &Some(NativeFee {
                        amount: message.fee.native_fee,
                        recipient: native_fee_recipient.clone(),
                    }),
                );

                self.update_storage_balance(
                    predecessor_account_id,
                    required_balance,
                    env::attached_deposit(),
                );
            }
        }

        let token = self.get_token_id(&message.token);
        env::log_str(
            &Nep141LockerEvent::ClaimFeeEvent {
                transfer_message: message,
            }
            .to_log_string(),
        );

        if fee > 0 {
            PromiseOrValue::Promise(
                ext_token::ext(token)
                    .with_static_gas(FT_TRANSFER_GAS)
                    .with_attached_deposit(ONE_YOCTO)
                    .ft_transfer(fin_transfer.fee_recipient, U128(fee), None),
            )
        } else {
            PromiseOrValue::Value(())
        }
    }

    #[payable]
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
        let deployer = self
            .token_deployer_accounts
            .get(&chain)
            .unwrap_or_else(|| env::panic_str("ERR_DEPLOYER_NOT_SET"));
        let prefix = metadata.token_address.get_token_prefix();
        let token_id: AccountId = format!("{prefix}.{deployer}")
            .parse()
            .unwrap_or_else(|_| env::panic_str("ERR_PARSE_ACCOUNT"));

        let storage_usage = env::storage_usage();
        require!(
            self.token_id_to_address
                .insert(&(chain, token_id.clone()), &metadata.token_address)
                .is_none(),
            "ERR_TOKEN_EXIST"
        );
        require!(
            self.token_address_to_id
                .insert(&metadata.token_address, &token_id)
                .is_none(),
            "ERR_TOKEN_EXIST"
        );
        require!(self.deployed_tokens.insert(&token_id), "ERR_TOKEN_EXIST");
        let required_deposit = env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into())
            .saturating_add(BRIDGE_TOKEN_INIT_BALANCE);

        require!(
            attached_deposit >= required_deposit,
            "ERROR: The deposit is not sufficient to cover the storage."
        );

        ext_deployer::ext(deployer)
            .with_static_gas(DEPLOY_TOKEN_GAS)
            .with_attached_deposit(BRIDGE_TOKEN_INIT_BALANCE)
            .deploy_token(
                token_id,
                BasicMetadata {
                    name: metadata.name,
                    symbol: metadata.symbol,
                    decimals: metadata.decimals,
                },
            )
    }

    #[payable]
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

    pub fn get_token_address(
        &self,
        chain_kind: ChainKind,
        token: AccountId,
    ) -> Option<OmniAddress> {
        self.token_id_to_address.get(&(chain_kind, token))
    }

    pub fn get_transfer_message(&self, nonce: U128) -> TransferMessage {
        self.pending_transfers
            .get(&nonce.0)
            .map(storage::TransferMessageStorage::into_main)
            .map(|m| m.message)
            .sdk_expect("The transfer does not exist")
    }

    pub fn get_transfer_message_storage(&self, nonce: U128) -> TransferMessageStorageValue {
        self.pending_transfers
            .get(&nonce.0)
            .map(storage::TransferMessageStorage::into_main)
            .sdk_expect("The transfer does not exist")
    }

    pub fn is_transfer_finalised(&self, chain: ChainKind, nonce: U128) -> bool {
        self.finalised_transfers.contains_key(&(chain, nonce.0))
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
    pub fn add_deployed_tokens(&mut self, tokens: Vec<(OmniAddress, AccountId)>) {
        for (token_address, token_id) in tokens {
            self.deployed_tokens.insert(&token_id);
            self.token_address_to_id.insert(&token_address, &token_id);
            self.token_id_to_address.insert(
                &(token_address.get_chain(), token_id.clone()),
                &token_address,
            );
        }
    }

    #[access_control_any(roles(Role::DAO, Role::MetadataManager))]
    pub fn set_token_metadata(
        &mut self,
        token: AccountId,
        name: Option<String>,
        symbol: Option<String>,
        decimals: Option<u8>,
        icon: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
    ) -> Promise {
        ext_token::ext(token)
            .with_static_gas(SET_METADATA_GAS)
            .set_metadata(name, symbol, reference, reference_hash, decimals, icon)
    }
}

impl Contract {
    fn get_token_id(&self, address: &OmniAddress) -> AccountId {
        if let OmniAddress::Near(token_account_id) = address {
            token_account_id.clone()
        } else {
            self.token_address_to_id
                .get(address)
                .sdk_expect("ERR_TOKEN_NOT_REGISTERED")
        }
    }

    fn check_or_pay_ft_storage(
        mut main_promise: Promise,
        args: &StorageDepositArgs,
        attached_deposit: &mut NearToken,
    ) -> Promise {
        for (account, is_storage_deposit) in &args.accounts {
            let promise = if *is_storage_deposit {
                *attached_deposit = attached_deposit
                    .checked_sub(NEP141_DEPOSIT)
                    .sdk_expect("The attached deposit is less than required");
                ext_token::ext(args.token.clone())
                    .with_static_gas(STORAGE_DEPOSIT_GAS)
                    .with_attached_deposit(NEP141_DEPOSIT)
                    .storage_deposit(Some(account.clone()), Some(true))
            } else {
                ext_token::ext(args.token.clone())
                    .with_static_gas(STORAGE_BALANCE_OF_GAS)
                    .with_attached_deposit(NO_DEPOSIT)
                    .storage_balance_of(Some(account.clone()))
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
        nonce: u128,
        transfer_message: TransferMessage,
        message_owner: AccountId,
    ) -> Option<Vec<u8>> {
        self.pending_transfers.insert_raw(
            &borsh::to_vec(&nonce).sdk_expect("ERR_BORSH"),
            &TransferMessageStorage::encode_borsh(transfer_message, message_owner)
                .sdk_expect("ERR_BORSH"),
        )
    }

    fn add_transfer_message(
        &mut self,
        nonce: u128,
        transfer_message: TransferMessage,
        message_owner: AccountId,
    ) -> NearToken {
        let storage_usage = env::storage_usage();
        require!(
            self.insert_raw_transfer(nonce, transfer_message, message_owner)
                .is_none(),
            "ERR_KEY_EXIST"
        );
        env::storage_byte_cost().saturating_mul((env::storage_usage() - storage_usage).into())
    }

    fn remove_transfer_message(&mut self, nonce: u128) -> TransferMessage {
        let storage_usage = env::storage_usage();
        let transfer = self
            .pending_transfers
            .remove(&nonce)
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

    fn add_fin_transfer(
        &mut self,
        transfer_id: &TransferId,
        native_token_fee: &Option<NativeFee>,
    ) -> NearToken {
        let storage_usage = env::storage_usage();
        require!(
            self.finalised_transfers
                .insert(transfer_id, native_token_fee)
                .is_none(),
            "The transfer is already finalised"
        );
        env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into())
    }

    fn update_fin_transfer(
        &mut self,
        transfer_id: &TransferId,
        native_token_fee: &Option<NativeFee>,
    ) -> NearToken {
        let storage_usage = env::storage_usage();
        self.finalised_transfers
            .insert(transfer_id, native_token_fee);
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

    fn refund(account_id: AccountId, amount: NearToken) {
        if !amount.is_zero() {
            Promise::new(account_id).transfer(amount);
        }
    }
}
