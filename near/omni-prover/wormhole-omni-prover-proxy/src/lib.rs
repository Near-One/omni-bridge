use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{
    env, ext_contract, near_bindgen, require, AccountId, Gas, PanicOnDefault, Promise, PromiseError,
};
use omni_types::prover_args::WormholeVerifyProofArgs;
use omni_types::prover_result::{ProofKind, ProverResult};

mod byte_utils;
mod parsed_vaa;

/// Gas to call `verify_log_entry` on prover.
pub const VERIFY_LOG_ENTRY_GAS: Gas = Gas::from_tgas(50);

#[ext_contract(ext_prover)]
pub trait Prover {
    fn verify_vaa(&self, vaa: &str) -> u32;
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct WormholeOmniProverProxy {
    pub prover_account: AccountId,
}

#[near_bindgen]
impl WormholeOmniProverProxy {
    #[init]
    #[private]
    #[must_use]
    pub fn init(prover_account: AccountId) -> Self {
        Self { prover_account }
    }

    pub fn verify_proof(&self, #[serializer(borsh)] input: &[u8]) -> Promise {
        let args = WormholeVerifyProofArgs::try_from_slice(input)
            .unwrap_or_else(|_| env::panic_str("ErrorOnArgsParsing"));

        env::log_str(&args.vaa.to_string());

        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_LOG_ENTRY_GAS)
            .verify_vaa(&args.vaa)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(VERIFY_LOG_ENTRY_GAS)
                    .verify_vaa_callback(args.proof_kind, args.vaa),
            )
    }

    /// # Panics
    ///
    /// This function will panic in the following situations:
    /// - If the `vaa` string cannot be decoded as a valid hexadecimal string.
    /// - If the `ParsedVAA::parse` function fails to parse the decoded VAA data.
    /// - If the `proof_kind` doesn't match the first byte of the VAA payload.
    #[private]
    #[handle_result]
    pub fn verify_vaa_callback(
        &mut self,
        proof_kind: ProofKind,
        vaa: String,
        #[callback_result] gov_idx: &Result<u32, PromiseError>,
    ) -> Result<ProverResult, String> {
        if gov_idx.is_err() {
            return Err("Proof is not valid!".to_owned());
        }

        let h = hex::decode(vaa).expect("invalidVaa");
        let parsed_vaa = parsed_vaa::ParsedVAA::parse(&h);

        require!(
            proof_kind as u8 == parsed_vaa.payload[0],
            "Invalid proof kind"
        );

        match proof_kind {
            ProofKind::InitTransfer => Ok(ProverResult::InitTransfer(parsed_vaa.try_into()?)),
            ProofKind::FinTransfer => Ok(ProverResult::FinTransfer(parsed_vaa.try_into()?)),
            ProofKind::DeployToken => Ok(ProverResult::DeployToken(parsed_vaa.try_into()?)),
        }
    }
}
