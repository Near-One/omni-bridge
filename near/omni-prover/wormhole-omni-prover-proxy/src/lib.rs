use near_sdk::borsh::BorshDeserialize;
use near_sdk::{
    env, ext_contract, near, near_bindgen, require, AccountId, Gas, PanicOnDefault, Promise,
    PromiseError,
};
use omni_types::prover_args::WormholeVerifyProofArgs;
use omni_types::prover_result::{ProofKind, ProverResult};

mod byte_utils;
mod parsed_vaa;

pub const VERIFY_VAA_GAS: Gas = Gas::from_tgas(10);
pub const VERIFY_VAA_CALLBACK_GAS: Gas = Gas::from_tgas(5);

#[ext_contract(ext_prover)]
pub trait Prover {
    fn verify_vaa(&self, vaa: &str) -> u32;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct WormholeOmniProverProxy {
    pub prover_account: AccountId,
}

#[near_bindgen]
impl WormholeOmniProverProxy {
    #[init]
    #[private]
    #[must_use]
    pub const fn init(prover_account: AccountId) -> Self {
        Self { prover_account }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> Promise {
        let args = WormholeVerifyProofArgs::try_from_slice(&input)
            .unwrap_or_else(|_| env::panic_str("ErrorOnArgsParsing"));

        env::log_str(&args.vaa);

        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_VAA_GAS)
            .verify_vaa(&args.vaa)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(VERIFY_VAA_CALLBACK_GAS)
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
    #[result_serializer(borsh)]
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
            u8::from(proof_kind) == parsed_vaa.payload[0],
            "Invalid proof kind"
        );

        match proof_kind {
            ProofKind::InitTransfer => Ok(ProverResult::InitTransfer(parsed_vaa.try_into()?)),
            ProofKind::FinTransfer => Ok(ProverResult::FinTransfer(parsed_vaa.try_into()?)),
            ProofKind::DeployToken => Ok(ProverResult::DeployToken(parsed_vaa.try_into()?)),
            ProofKind::LogMetadata => Ok(ProverResult::LogMetadata(parsed_vaa.try_into()?)),
        }
    }
}
