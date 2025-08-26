use near_sdk::borsh::BorshDeserialize;
use near_sdk::{near, AccountId};
use omni_types::prover_result::ProverResult;
use omni_types::ChainKind;

#[near(contract_state)]
pub struct EvmProver {
    /// Kept only for interface compatibility; unused in the mock.
    pub light_client: AccountId,
    /// Which EVM family this prover is parsing for (needed by `parse_evm_event`).
    pub chain_kind: ChainKind,
}

impl Default for EvmProver {
    fn default() -> Self {
        Self {
            light_client: "light_client".parse().unwrap(),
            chain_kind: ChainKind::Eth,
        }
    }
}

#[near]
impl EvmProver {
    /// MOCK: no block-hash check, no trie verification.
    /// Decodes args and returns the parsed event as `ProverResult` immediately.
    #[allow(clippy::needless_pass_by_value)]
    #[result_serializer(borsh)]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> ProverResult {
        ProverResult::try_from_slice(&input).unwrap()
    }
}
