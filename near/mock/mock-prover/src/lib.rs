use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{near, AccountId};
use omni_types::prover_args::{ProverId, VerifyProofArgs};
use omni_types::prover_result::ProverResult;

#[derive(BorshSerialize, near_sdk::BorshStorageKey)]
enum StorageKey {
    RegisteredProvers,
}

#[near(contract_state)]
pub struct OmniProver {
    provers: UnorderedMap<ProverId, AccountId>,
}

impl Default for OmniProver {
    fn default() -> Self {
        Self {
            provers: near_sdk::collections::UnorderedMap::new(StorageKey::RegisteredProvers),
        }
    }
}

#[near]
impl OmniProver {
    pub fn add_prover(&mut self, prover_id: ProverId, account_id: AccountId) {
        self.provers.insert(&prover_id, &account_id);
    }

    pub fn remove_prover(&mut self, prover_id: ProverId) {
        self.provers.remove(&prover_id);
    }

    pub fn get_provers(&self) -> Vec<(ProverId, AccountId)> {
        self.provers.iter().collect::<Vec<_>>()
    }

    #[result_serializer(borsh)]
    pub fn verify_proof(&self, #[serializer(borsh)] args: VerifyProofArgs) -> ProverResult {
        ProverResult::try_from_slice(&args.prover_args).unwrap()
    }
}
