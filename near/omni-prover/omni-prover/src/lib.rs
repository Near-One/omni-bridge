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
use omni_types::prover_args::VerifyProofArgs;
use omni_types::prover_result::ProverResult;
use omni_types::ChainKind;

const OUTER_VERIFY_PROOF_GAS: Gas = Gas::from_tgas(10);

#[ext_contract(ext_omni_prover_proxy)]
pub trait Prover {
    #[result_serializer(borsh)]
    fn verify_proof(&self, #[serializer(borsh)] proof: Vec<u8>) -> ProverResult;
}

#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    PauseManager,
    UpgradableCodeStager,
    UpgradableCodeDeployer,
    DAO,
    BridgesManager,
    UnrestrictedValidateProof,
}

#[derive(BorshSerialize, near_sdk::BorshStorageKey)]
enum StorageKey {
    RegisteredBridges,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, Pausable, Upgradable)]
#[access_control(role_type(Role))]
#[pausable(manager_roles(Role::PauseManager, Role::DAO))]
#[upgradable(access_control_roles(
    code_stagers(Role::UpgradableCodeStager, Role::DAO),
    code_deployers(Role::UpgradableCodeDeployer, Role::DAO),
    duration_initializers(Role::DAO),
    duration_update_stagers(Role::DAO),
    duration_update_appliers(Role::DAO),
))]
pub struct OmniProver {
    bridges: UnorderedMap<ChainKind, AccountId>,
}

#[near_bindgen]
impl OmniProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init() -> Self {
        let mut contract = Self {
            bridges: near_sdk::collections::UnorderedMap::new(StorageKey::RegisteredBridges),
        };

        contract.acl_init_super_admin(near_sdk::env::predecessor_account_id());
        contract
    }

    #[access_control_any(roles(Role::BridgesManager, Role::DAO))]
    pub fn set_bridge(&mut self, chain_kind: ChainKind, account_id: AccountId) {
        self.bridges.insert(&chain_kind, &account_id);
    }

    #[access_control_any(roles(Role::BridgesManager, Role::DAO))]
    pub fn remove_bridge(&mut self, chain_kind: ChainKind) {
        self.bridges.remove(&chain_kind);
    }

    pub fn get_bridges_list(&self) -> Vec<(ChainKind, AccountId)> {
        self.bridges.iter().collect::<Vec<_>>()
    }

    #[pause(except(roles(Role::UnrestrictedValidateProof, Role::DAO)))]
    pub fn verify_proof(&self, #[serializer(borsh)] proof: Vec<u8>) -> Promise {
        let input = VerifyProofArgs::try_from_slice(&proof)
            .unwrap_or_else(|_| env::panic_str("ErrorOnVerifyProofInputParsing"));
        let bridge_account_id = self
            .bridges
            .get(&input.chain_kind)
            .unwrap_or_else(|| env::panic_str("ProverIdNotRegistered"));

        ext_omni_prover_proxy::ext(bridge_account_id)
            .with_static_gas(env::prepaid_gas().saturating_sub(OUTER_VERIFY_PROOF_GAS))
            .with_attached_deposit(NearToken::from_near(0))
            .verify_proof(input.prover_args)
    }
}
