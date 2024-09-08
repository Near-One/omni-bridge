use near_plugins::{
    access_control, access_control_any, pause, AccessControlRole, AccessControllable, Pausable,
    Upgradable,
};

use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, LookupSet};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near, require, serde_json, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, Promise, PromiseError, PromiseOrValue, PromiseResult,
};
use omni_types::locker_args::{BindTokenArgs, ClaimFeeArgs, FinTransferArgs, StorageDepositArgs};
use omni_types::mpc_types::SignatureResponse;
use omni_types::near_events::Nep141LockerEvent;
use omni_types::prover_args::VerifyProofArgs;
use omni_types::prover_result::ProverResult;
use omni_types::{
    ChainKind, MetadataPayload, NearRecipient, Nonce, OmniAddress, SignRequest, TransferMessage,
    TransferMessagePayload, UpdateFee,
};

const LOG_METADATA_GAS: Gas = Gas::from_tgas(10);
const LOG_METADATA_CALLBCAK_GAS: Gas = Gas::from_tgas(260);
const MPC_SIGNING_GAS: Gas = Gas::from_tgas(250);
const SIGN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const VERIFY_POOF_GAS: Gas = Gas::from_tgas(50);
const CLAIM_FEE_CALLBACK_GAS: Gas = Gas::from_tgas(50);
const BIND_TOKEN_CALLBACK_GAS: Gas = Gas::from_tgas(25);
const FT_TRANSFER_CALL_GAS: Gas = Gas::from_tgas(50);
const FT_TRANSFER_GAS: Gas = Gas::from_tgas(5);
const STORAGE_BALANCE_OF_GAS: Gas = Gas::from_tgas(3);
const STORAGE_DEPOSIT_GAS: Gas = Gas::from_tgas(3);
const NO_DEPOSIT: NearToken = NearToken::from_near(0);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const NEP141_DEPOSIT: NearToken = NearToken::from_yoctonear(1250000000000000000000);

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    PendingTransfers,
    Factories,
    FinalisedTransfers,
    TokensMapping,
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
    pub finalised_transfers: LookupSet<(ChainKind, Nonce)>,
    pub tokens_to_address_mapping: LookupMap<(ChainKind, AccountId), OmniAddress>,
    pub mpc_signer: AccountId,
    pub current_nonce: Nonce,
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
        self.current_nonce += 1;
        let transfer_message = TransferMessage {
            origin_nonce: U128(self.current_nonce),
            token: env::predecessor_account_id(),
            amount,
            recipient: msg.parse().unwrap(),
            fee: U128(0), // TODO get fee from msg
            sender: OmniAddress::Near(sender_id.to_string()),
        };

        self.pending_transfers
            .insert(&self.current_nonce, &transfer_message);

        env::log_str(&Nep141LockerEvent::InitTransferEvent { transfer_message }.to_log_string());
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
            finalised_transfers: LookupSet::new(StorageKey::FinalisedTransfers),
            tokens_to_address_mapping: LookupMap::new(StorageKey::TokensMapping),
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
                Self::ext(env::current_account_id())
                    .with_static_gas(LOG_METADATA_CALLBCAK_GAS)
                    .with_attached_deposit(env::attached_deposit())
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

    pub fn update_transfer_fee(&mut self, nonce: U128, fee: UpdateFee) {
        match fee {
            UpdateFee::Fee(fee) => {
                let mut message = self.get_transfer_message(nonce);

                require!(
                    OmniAddress::Near(env::predecessor_account_id().to_string()) == message.sender,
                    "Only sender can update fee"
                );

                message.fee = fee;
                self.pending_transfers.insert(&nonce.0, &message);
            }
            UpdateFee::Proof(_) => env::panic_str("TODO"),
        }
    }

    #[payable]
    pub fn sign_transfer(&mut self, nonce: U128, fee_recipient: Option<AccountId>) -> Promise {
        let transfer_message = self.get_transfer_message(nonce);
        let transfer_payload = TransferMessagePayload {
            nonce,
            token: transfer_message.token,
            amount: U128(transfer_message.amount.0 - transfer_message.fee.0),
            recipient: transfer_message.recipient,
            fee_recipient,
        };

        let payload = near_sdk::env::keccak256_array(&borsh::to_vec(&transfer_payload).unwrap());

        ext_signer::ext(self.mpc_signer.clone())
            .with_static_gas(MPC_SIGNING_GAS)
            .with_attached_deposit(env::attached_deposit())
            .sign(SignRequest {
                payload,
                path: "bridge-1".to_owned(),
                key_version: 0,
            })
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(SIGN_TRANSFER_CALLBACK_GAS)
                    .sign_transfer_callback(transfer_payload),
            )
    }

    #[private]
    pub fn sign_transfer_callback(
        &mut self,
        #[callback_result] call_result: Result<SignatureResponse, PromiseError>,
        #[serializer(borsh)] message_payload: TransferMessagePayload,
    ) {
        if let Ok(signature) = call_result {
            let nonce = message_payload.nonce;
            let transfer_message = self.get_transfer_message(nonce);
            if transfer_message.fee.0 == 0 {
                self.pending_transfers.remove(&nonce.0);
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
            .with_static_gas(VERIFY_POOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(VerifyProofArgs {
                prover_id: args.chain_kind.as_ref().to_owned(),
                prover_args: args.prover_args,
            });

        Self::check_or_pay_ft_storage(
            main_promise,
            &args.storage_deposit_args,
            env::attached_deposit(),
        )
        .then(
            Self::ext(env::current_account_id())
                .with_attached_deposit(NO_DEPOSIT)
                .with_static_gas(CLAIM_FEE_CALLBACK_GAS)
                .fin_transfer_callback(args.storage_deposit_args),
        )
    }

    #[private]
    pub fn fin_transfer_callback(
        &mut self,
        #[serializer(borsh)] storage_deposit_args: StorageDepositArgs,
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
        // TODO: pay for storage
        require!(
            self.finalised_transfers.insert(&(
                transfer_message.get_origin_chain(),
                transfer_message.origin_nonce.0,
            )) == true,
            "The transfer is already finalised"
        );

        if let OmniAddress::Near(recipient) = &transfer_message.recipient {
            let recipient: NearRecipient = recipient
                .parse()
                .unwrap_or_else(|_| env::panic_str("Failed to parse recipient"));

            require!(
                transfer_message.token == storage_deposit_args.token,
                "Invalid token"
            );
            require!(
                Self::check_storage_balance_result(1)
                    && storage_deposit_args.accounts[0].0 == recipient.target,
                "The transfer recipient was omitted"
            );

            let amount_to_transfer = U128(transfer_message.amount.0 - transfer_message.fee.0);
            let mut promise = match recipient.message {
                Some(message) => ext_token::ext(transfer_message.token.clone())
                    .with_static_gas(FT_TRANSFER_CALL_GAS)
                    .with_attached_deposit(ONE_YOCTO)
                    .ft_transfer_call(recipient.target, amount_to_transfer, None, message),
                None => ext_token::ext(transfer_message.token.clone())
                    .with_static_gas(FT_TRANSFER_GAS)
                    .with_attached_deposit(ONE_YOCTO)
                    .ft_transfer(recipient.target, amount_to_transfer, None),
            };

            if transfer_message.fee.0 > 0 {
                let signer = env::signer_account_id();
                require!(
                    Self::check_storage_balance_result(2)
                        && storage_deposit_args.accounts[1].0 == signer,
                    "The fee recipient was omitted"
                );
                promise = promise.and(
                    ext_token::ext(transfer_message.token.clone())
                        .with_static_gas(FT_TRANSFER_GAS)
                        .with_attached_deposit(ONE_YOCTO)
                        .ft_transfer(signer, transfer_message.fee, None),
                );
            }

            env::log_str(
                &Nep141LockerEvent::FinTransferEvent {
                    nonce: None,
                    transfer_message,
                }
                .to_log_string(),
            );
            promise.into()
        } else {
            self.current_nonce += 1;
            self.pending_transfers
                .insert(&self.current_nonce, &transfer_message);

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

    pub fn claim_fee(&self, #[serializer(borsh)] args: ClaimFeeArgs) -> Promise {
        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_POOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(VerifyProofArgs {
                prover_id: args.chain_kind.as_ref().to_owned(),
                prover_args: args.prover_args,
            })
            .then(
                Self::ext(env::current_account_id())
                    .with_attached_deposit(env::attached_deposit())
                    .with_static_gas(CLAIM_FEE_CALLBACK_GAS)
                    .claim_fee_callback(),
            )
    }

    #[private]
    pub fn claim_fee_callback(
        &mut self,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProverResult, PromiseError>,
    ) -> Promise {
        let Ok(ProverResult::FinTransfer(fin_transfer)) = call_result else {
            env::panic_str("Invalid proof message")
        };

        let message = self.get_transfer_message(fin_transfer.nonce);
        self.pending_transfers.remove(&fin_transfer.nonce.0);
        require!(
            self.factories
                .get(&fin_transfer.emitter_address.get_chain())
                == Some(fin_transfer.emitter_address),
            "Unknown factory"
        );

        let fee = message.amount.0 - fin_transfer.amount.0;

        ext_token::ext(message.token)
            .with_static_gas(LOG_METADATA_GAS)
            .ft_transfer(fin_transfer.fee_recipient, U128(fee), None)
    }

    #[payable]
    pub fn bind_token(&mut self, #[serializer(borsh)] args: BindTokenArgs) -> Promise {
        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_POOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(VerifyProofArgs {
                prover_id: args.chain_kind.as_ref().to_owned(),
                prover_args: args.prover_args,
            })
            .then(
                Self::ext(env::current_account_id())
                    .with_attached_deposit(env::attached_deposit())
                    .with_static_gas(BIND_TOKEN_CALLBACK_GAS)
                    .bind_token_callback(),
            )
    }

    #[private]
    pub fn bind_token_callback(
        &mut self,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProverResult, PromiseError>,
    ) {
        let Ok(ProverResult::DeployToken(deploy_token)) = call_result else {
            env::panic_str("Invalid proof message")
        };

        self.tokens_to_address_mapping.insert(
            &(deploy_token.token_address.get_chain(), deploy_token.token),
            &deploy_token.token_address,
        );
    }

    pub fn get_token_address(
        &self,
        chain_kind: ChainKind,
        token: AccountId,
    ) -> Option<OmniAddress> {
        self.tokens_to_address_mapping.get(&(chain_kind, token))
    }

    pub fn get_transfer_message(&self, nonce: U128) -> TransferMessage {
        self.pending_transfers
            .get(&nonce.0)
            .unwrap_or_else(|| panic!("The transfer does not exist"))
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn add_factory(&mut self, address: OmniAddress) {
        self.factories.insert(&(&address).into(), &address);
    }
}

impl Contract {
    fn check_or_pay_ft_storage(
        mut main_promise: Promise,
        args: &StorageDepositArgs,
        mut attached_deposit: NearToken,
    ) -> Promise {
        for (account, is_storage_deposit) in &args.accounts {
            let promise = if *is_storage_deposit {
                attached_deposit =
                    attached_deposit
                        .checked_sub(NEP141_DEPOSIT)
                        .unwrap_or_else(|| {
                            env::panic_str("The attached deposit is less than required")
                        });
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
        match env::promise_result(result_idx) {
            PromiseResult::Successful(data) => {
                serde_json::from_slice::<Option<StorageBalance>>(&data)
                    .ok()
                    .flatten()
                    .is_some()
            }
            _ => false,
        }
    }

    fn decode_prover_result(result_idx: u64) -> Result<ProverResult, PromiseError> {
        match env::promise_result(result_idx) {
            PromiseResult::Successful(data) => Ok(ProverResult::try_from_slice(&data)
                .unwrap_or_else(|_| env::panic_str("Invalid proof"))),
            PromiseResult::Failed => Err(PromiseError::Failed),
        }
    }
}
