use near_sdk::borsh::BorshDeserialize;
use near_sdk::{near, AccountId};
use omni_types::evm::events::parse_evm_event;
use omni_types::prover_args::EvmVerifyProofArgs;
use omni_types::prover_result::{ProofKind, ProverResult};
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
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_proof(
        &self,
        #[serializer(borsh)] input: Vec<u8>,
    ) -> Result<ProverResult, String> {
        let args =
            EvmVerifyProofArgs::try_from_slice(&input).map_err(|_| "ERR_PARSE_ARGS".to_string())?;

        let log_entry_data = args.proof.log_entry_data;

        let out = match args.proof_kind {
            ProofKind::InitTransfer => {
                ProverResult::InitTransfer(parse_evm_event(self.chain_kind, log_entry_data)?)
            }
            ProofKind::FinTransfer => {
                ProverResult::FinTransfer(parse_evm_event(self.chain_kind, log_entry_data)?)
            }
            ProofKind::DeployToken => {
                ProverResult::DeployToken(parse_evm_event(self.chain_kind, log_entry_data)?)
            }
            ProofKind::LogMetadata => {
                ProverResult::LogMetadata(parse_evm_event(self.chain_kind, log_entry_data)?)
            }
        };

        Ok(out)
    }
}
