use near_plugins::{
    access_control, access_control_any, pause, AccessControlRole, AccessControllable, Pausable,
    Upgradable,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near, near_bindgen, AccountId, Gas, NearToken, PanicOnDefault, Promise,
};
use omni_types::prover_args::{ProverId, VerifyProofArgs};

const OUTER_VERIFY_PROOF_GAS: Gas = Gas::from_tgas(10);

#[ext_contract(ext_omni_prover_proxy)]
pub trait Prover {
    fn verify_proof(&self, #[serializer(borsh)] proof: Vec<u8>);
}

#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    PauseManager,
    UnpauseManager,
    UpgradableCodeStager,
    UpgradableCodeDeployer,
    DAO,
    ProversManager,
    UnrestrictedValidateProof,
}

#[derive(BorshSerialize, near_sdk::BorshStorageKey)]
enum StorageKey {
    RegisteredProvers,
}

#[near(contract_state)]
#[derive(PanicOnDefault, Pausable, Upgradable)]
#[access_control(role_type(Role))]
#[pausable(
    pause_roles(Role::PauseManager, Role::DAO),
    unpause_roles(Role::UnpauseManager, Role::DAO)
)]
#[upgradable(access_control_roles(
    code_stagers(Role::UpgradableCodeStager, Role::DAO),
    code_deployers(Role::UpgradableCodeDeployer, Role::DAO),
    duration_initializers(Role::DAO),
    duration_update_stagers(Role::DAO),
    duration_update_appliers(Role::DAO),
))]
pub struct OmniProver {
    provers: UnorderedMap<ProverId, AccountId>,
}

#[near_bindgen]
impl OmniProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init() -> Self {
        let mut contract = Self {
            provers: near_sdk::collections::UnorderedMap::new(StorageKey::RegisteredProvers),
        };

        contract.acl_init_super_admin(near_sdk::env::predecessor_account_id());
        contract
    }

    #[access_control_any(roles(Role::ProversManager, Role::DAO))]
    pub fn add_prover(&mut self, prover_id: ProverId, account_id: AccountId) {
        self.provers.insert(&prover_id, &account_id);
    }

    #[access_control_any(roles(Role::ProversManager, Role::DAO))]
    pub fn remove_prover(&mut self, prover_id: ProverId) {
        self.provers.remove(&prover_id);
    }

    #[must_use]
    pub fn get_provers(&self) -> Vec<(ProverId, AccountId)> {
        self.provers.iter().collect::<Vec<_>>()
    }

    #[pause(except(roles(Role::UnrestrictedValidateProof, Role::DAO)))]
    pub fn verify_proof(&self, #[serializer(borsh)] args: VerifyProofArgs) -> Promise {
        let prover_account_id = self
            .provers
            .get(&args.prover_id)
            .unwrap_or_else(|| env::panic_str("ProverIdNotRegistered"));

        ext_omni_prover_proxy::ext(prover_account_id)
            .with_static_gas(env::prepaid_gas().saturating_sub(OUTER_VERIFY_PROOF_GAS))
            .with_attached_deposit(NearToken::from_near(0))
            .verify_proof(args.prover_args)
    }
}
