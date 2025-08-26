use near_sdk::borsh::BorshDeserialize;
use near_sdk::near;
use omni_types::prover_result::ProverResult;

#[near(contract_state)]
#[derive(Default)]
pub struct Prover {}

#[near]
impl Prover {
    /// # Panics
    ///
    /// This function will panic if the prover args are not valid.
    #[allow(clippy::needless_pass_by_value)]
    #[result_serializer(borsh)]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> ProverResult {
        ProverResult::try_from_slice(&input).unwrap()
    }
}
