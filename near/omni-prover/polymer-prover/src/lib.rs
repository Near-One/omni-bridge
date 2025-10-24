use near_sdk::borsh::BorshDeserialize;
use near_sdk::{
    env, ext_contract, near, near_bindgen, AccountId, Gas, PanicOnDefault, Promise,
    PromiseError,
};
use omni_types::polymer::events::parse_polymer_event_by_kind;
use omni_types::prover_args::PolymerVerifyProofArgs;
use omni_types::prover_result::{ProofKind, ProverResult};

pub const VERIFY_PROOF_GAS: Gas = Gas::from_tgas(15);
pub const VERIFY_PROOF_CALLBACK_GAS: Gas = Gas::from_tgas(5);

/// Interface to Polymer's CrossL2ProverV2 contract deployed on NEAR
/// This contract validates IAVL proofs from Polymer Hub
#[ext_contract(ext_polymer_verifier)]
pub trait PolymerVerifier {
    /// Validates a Polymer proof and returns event data
    /// Returns: (chainId, emittingContract, topics, unindexedData)
    fn validate_event(&self, proof: Vec<u8>) -> (u32, String, Vec<u8>, Vec<u8>);
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct PolymerProver {
    /// Account ID of the deployed Polymer verifier contract on NEAR
    pub verifier_account: AccountId,
}

#[near_bindgen]
impl PolymerProver {
    #[init]
    #[private]
    #[must_use]
    pub const fn init(verifier_account: AccountId) -> Self {
        Self { verifier_account }
    }

    /// Main entry point: accepts proof bytes and delegates to Polymer verifier
    #[allow(clippy::needless_pass_by_value)]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> Promise {
        let args = PolymerVerifyProofArgs::try_from_slice(&input)
            .unwrap_or_else(|_| env::panic_str("ERR_PARSE_ARGS"));

        env::log_str(&format!(
            "Polymer proof verification: chain_id={}, block={}, log_index={}",
            args.src_chain_id, args.src_block_number, args.global_log_index
        ));

        // Call Polymer verifier contract
        ext_polymer_verifier::ext(self.verifier_account.clone())
            .with_static_gas(VERIFY_PROOF_GAS)
            .validate_event(args.proof.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(VERIFY_PROOF_CALLBACK_GAS)
                    .verify_proof_callback(
                        args.proof_kind,
                        args.src_chain_id,
                        args.src_block_number,
                        args.global_log_index,
                    ),
            )
    }

    /// Callback after Polymer verifier validates the proof
    /// Parses the validated event data into ProverResult
    #[private]
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_proof_callback(
        &mut self,
        #[serializer(borsh)] proof_kind: ProofKind,
        #[serializer(borsh)] src_chain_id: u64,
        #[serializer(borsh)] _src_block_number: u64,
        #[serializer(borsh)] _global_log_index: u64,
        #[callback_result] validation_result: &Result<(u32, String, Vec<u8>, Vec<u8>), PromiseError>,
    ) -> Result<ProverResult, String> {
        let (chain_id, emitting_contract, topics, unindexed_data) =
            validation_result
                .as_ref()
                .map_err(|_| "Polymer proof validation failed".to_owned())?;

        // Verify the chain_id matches what we requested
        if u64::from(*chain_id) != src_chain_id {
            return Err(format!(
                "Chain ID mismatch: expected {}, got {}",
                src_chain_id, chain_id
            ));
        }

        env::log_str(&format!(
            "Proof validated: contract={}, topics_len={}, data_len={}",
            emitting_contract,
            topics.len(),
            unindexed_data.len()
        ));

        // Parse event based on proof kind
        self.parse_polymer_event(
            proof_kind,
            emitting_contract,
            topics,
            unindexed_data,
            src_chain_id,
        )
    }

    /// Parse Polymer-validated event data into ProverResult
    fn parse_polymer_event(
        &self,
        proof_kind: ProofKind,
        emitting_contract: &str,
        topics: &[u8],
        unindexed_data: &[u8],
        chain_id: u64,
    ) -> Result<ProverResult, String> {
        // Verify minimum topics length (event signature must be present)
        if topics.len() < 32 {
            return Err("Invalid topics length: must be at least 32 bytes".to_owned());
        }

        parse_polymer_event_by_kind(
            proof_kind,
            chain_id,
            emitting_contract,
            topics,
            unindexed_data,
        )
    }
}
