use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near_bindgen, AccountId, Gas, PanicOnDefault, Promise, PromiseError,
};
use omni_types::evm_events::parse_evm_event;
use omni_types::prover_types::{ProofKind, ProofResult};
use omni_types::ChainKind;

/// Gas to call verify_log_entry on prover.
pub const VERIFY_LOG_ENTRY_GAS: Gas = Gas::from_tgas(50);

#[ext_contract(ext_prover)]
pub trait Prover {
    #[result_serializer(borsh)]
    fn verify_log_entry(
        &self,
        #[serializer(borsh)] log_index: u64,
        #[serializer(borsh)] log_entry_data: Vec<u8>,
        #[serializer(borsh)] receipt_index: u64,
        #[serializer(borsh)] receipt_data: Vec<u8>,
        #[serializer(borsh)] header_data: Vec<u8>,
        #[serializer(borsh)] proof: Vec<Vec<u8>>,
        #[serializer(borsh)] skip_bridge_call: bool,
    ) -> bool;
}

#[derive(Default, BorshDeserialize, BorshSerialize, Clone, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Proof {
    pub log_index: u64,
    pub log_entry_data: Vec<u8>,
    pub receipt_index: u64,
    pub receipt_data: Vec<u8>,
    pub header_data: Vec<u8>,
    pub proof: Vec<Vec<u8>>,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct RainbowOmniProverProxy {
    pub prover_account: AccountId,
    pub chain_kind: ChainKind,
}

#[near_bindgen]
impl RainbowOmniProverProxy {
    #[init]
    #[private]
    #[must_use]
    pub fn init(prover_account: AccountId, chain_kind: ChainKind) -> Self {
        Self {
            prover_account,
            chain_kind,
        }
    }

    pub fn verify_proof(
        &self,
        #[serializer(borsh)] kind: ProofKind,
        #[serializer(borsh)] msg: Vec<u8>,
    ) -> Promise {
        let proof =
            Proof::try_from_slice(&msg).unwrap_or_else(|_| env::panic_str("ErrorOnProofParsing"));

        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_LOG_ENTRY_GAS)
            .verify_log_entry(
                proof.log_index,
                proof.log_entry_data.clone(),
                proof.receipt_index,
                proof.receipt_data,
                proof.header_data,
                proof.proof,
                false, // Do not skip bridge call. This is only used for development and diagnostics.
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(VERIFY_LOG_ENTRY_GAS)
                    .verify_log_entry_callback(kind, proof.log_entry_data),
            )
    }

    #[private]
    #[handle_result]
    pub fn verify_log_entry_callback(
        &mut self,
        #[serializer(borsh)] kind: ProofKind,
        #[serializer(borsh)] log_entry_data: Vec<u8>,
        #[callback_result] is_valid: Result<bool, PromiseError>,
    ) -> Result<ProofResult, String> {
        if !is_valid.unwrap_or(false) {
            return Err("Proof is not valid!".to_owned());
        }

        match kind {
            ProofKind::InitTransfer => Ok(ProofResult::InitTransfer(parse_evm_event(
                self.chain_kind,
                log_entry_data,
            )?)),
            ProofKind::FinTransfer => Ok(ProofResult::FinTransfer(parse_evm_event(
                self.chain_kind,
                log_entry_data,
            )?)),
            ProofKind::DeployToken => Ok(ProofResult::DeployToken(parse_evm_event(
                self.chain_kind,
                log_entry_data,
            )?)),
        }
    }
}
