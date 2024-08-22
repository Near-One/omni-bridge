use near_plugins::{
    access_control, access_control_any, pause, AccessControlRole, AccessControllable, Pausable,
    Upgradable,
};

use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap; // TODO compare the perfomance with store
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault,
    Promise, PromiseError, PromiseOrValue,
};

mod types;
use types::*;

const LOG_METADATA_GAS: Gas = Gas::from_tgas(10);
const LOG_METADATA_CALLBCAK_GAS: Gas = Gas::from_tgas(30);
const MPC_SIGNING_GAS: Gas = Gas::from_tgas(250);
const SIGN_TRANSFER_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const VERIFY_POOF_GAS: Gas = Gas::from_tgas(50);
const FINISH_CLAIM_FEE_GAS: Gas = Gas::from_tgas(50);
const FT_TRANSFER_CALL_GAS: Gas = Gas::from_tgas(50);
const FT_TRANSFER_GAS: Gas = Gas::from_tgas(5);
const NO_DEPOSIT: NearToken = NearToken::from_near(0);

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
        #[callback_result] call_result: Result<SignatureResponse, PromiseError>,
        nonce: U128,
    );
    fn fin_transfer_callback(
        &self,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProofResult, PromiseError>,
    );
    fn claim_fee_callback(
        &self,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProofResult, PromiseError>,
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

#[ext_contract(ext_prover)]
pub trait Prover {
    #[result_serializer(borsh)]
    fn verify_proof(&self, #[serializer(borsh)] proof: Vec<u8>) -> ProofResult;
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
    pub fn sign_transfer(&mut self, nonce: U128, relayer: Option<OmniAddress>) -> Promise {
        let transfer_message = self.get_transfer_message(nonce);
        let withdraw_payload = TransferMessagePayload {
            nonce,
            token: transfer_message.token,
            amount: U128(transfer_message.amount.0 - transfer_message.fee.0),
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
        #[callback_result] call_result: Result<SignatureResponse, PromiseError>,
        nonce: U128,
    ) {
        if let Ok(_) = call_result {
            let transfer_message = self.get_transfer_message(nonce);

            if transfer_message.fee.0 == 0 {
                self.pending_transfers.remove(&nonce.0);
            }
        }
    }

    pub fn fin_transfer(&self, proof: Vec<u8>) -> Promise {
        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_POOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(proof)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_attached_deposit(env::attached_deposit())
                    .with_static_gas(FINISH_CLAIM_FEE_GAS)
                    .claim_fee_callback(),
            )
    }

    #[private]
    pub fn fin_transfer_callback(
        &mut self,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProofResult, PromiseError>,
    ) -> PromiseOrValue<U128> {
        let Ok(ProofResult::InitTransfer(transfer_message)) = call_result else {
            env::panic_str("Invalid proof message")
        };

        if let OmniAddress::Near(recipient) = transfer_message.recipient {
            let recipient: NearRecipient = recipient
                .parse()
                .unwrap_or_else(|_| env::panic_str("Failed to parse recipient"));

            let amount_to_transfer = U128(transfer_message.amount.0 - transfer_message.fee.0);
            // TODO transfer fee
            match recipient.message {
                Some(message) => ext_token::ext(transfer_message.token)
                    .with_static_gas(FT_TRANSFER_CALL_GAS)
                    .with_attached_deposit(NearToken::from_yoctonear(0))
                    .ft_transfer_call(recipient.target, amount_to_transfer, None, message)
                    .into(),
                None => ext_token::ext(transfer_message.token)
                    .with_static_gas(FT_TRANSFER_GAS)
                    .with_attached_deposit(NearToken::from_yoctonear(0))
                    .ft_transfer(recipient.target, amount_to_transfer, None)
                    .into(),
            }
        } else {
            self.current_nonce += 1;
            self.pending_transfers
                .insert(&self.current_nonce, &transfer_message);

            PromiseOrValue::Value(U128(self.current_nonce))
        }
    }

    pub fn claim_fee(&self, proof: Vec<u8>) -> Promise {
        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_POOF_GAS)
            .with_attached_deposit(NO_DEPOSIT)
            .verify_proof(proof)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_attached_deposit(env::attached_deposit())
                    .with_static_gas(FINISH_CLAIM_FEE_GAS)
                    .claim_fee_callback(),
            )
    }

    #[private]
    pub fn claim_fee_callback(
        &mut self,
        #[callback_result]
        #[serializer(borsh)]
        call_result: Result<ProofResult, PromiseError>,
    ) -> Promise {
        let Ok(ProofResult::FinTransfer(fin_transfer)) = call_result else {
            env::panic_str("Invalid proof message")
        };

        let message = self.get_transfer_message(fin_transfer.nonce);
        self.pending_transfers.remove(&fin_transfer.nonce.0);
        require!(
            self.factories.get(&fin_transfer.factory.get_chain()) == Some(fin_transfer.factory),
            "Unknown factory"
        );

        ext_token::ext(message.token)
            .with_static_gas(LOG_METADATA_GAS)
            .ft_transfer(fin_transfer.claim_recipient, message.fee, None)
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
