use near_plugins::{
    access_control, access_control_any, AccessControlRole, AccessControllable, Pausable, Upgradable,
};
use near_sdk::borsh::BorshDeserialize;
use near_sdk::json_types::Base58CryptoHash;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;
use near_sdk::{env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise};
use omni_types::BasicMetadata;

const BRIDGE_TOKEN_INIT_BALANCE: NearToken = NearToken::from_near(3);
const NO_DEPOSIT: NearToken = NearToken::from_near(0);
const OMNI_TOKEN_INIT_GAS: Gas = Gas::from_tgas(10);

const BRIDGE_TOKEN_BINARY: &[u8] = include_bytes!(env!("OMNI_TOKEN_WASM"));

#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    DAO,
    PauseManager,
    UnrestrictedDeposit,
    UpgradableCodeStager,
    UpgradableCodeDeployer,
    Controller,
}

#[near(contract_state)]
#[derive(Pausable, Upgradable, PanicOnDefault)]
#[pausable(manager_roles(Role::PauseManager))]
#[access_control(role_type(Role))]
#[upgradable(access_control_roles(
    code_stagers(Role::UpgradableCodeStager, Role::DAO),
    code_deployers(Role::UpgradableCodeDeployer, Role::DAO),
    duration_initializers(Role::DAO),
    duration_update_stagers(Role::DAO),
    duration_update_appliers(Role::DAO),
))]
pub struct TokenDeployer {}

#[near]
impl TokenDeployer {
    #[init]
    pub fn new(controller: AccountId, dao: AccountId) -> Self {
        let mut contract = Self {};

        contract.acl_init_super_admin(near_sdk::env::predecessor_account_id());
        contract.acl_grant_role(Role::DAO.into(), dao.clone());
        contract.acl_grant_role(Role::Controller.into(), controller);
        contract.acl_transfer_super_admin(dao);
        contract
    }

    #[payable]
    #[access_control_any(roles(Role::Controller))]
    pub fn deploy_token(&mut self, account_id: AccountId, metadata: &BasicMetadata) -> Promise {
        require!(
            env::attached_deposit() >= BRIDGE_TOKEN_INIT_BALANCE,
            "ERR_NOT_ENOUGH_ATTACHED_BALANCE"
        );

        Promise::new(account_id)
            .create_account()
            .transfer(BRIDGE_TOKEN_INIT_BALANCE)
            .deploy_contract(BRIDGE_TOKEN_BINARY.to_vec())
            .function_call(
                "new".to_string(),
                json!({"controller": env::predecessor_account_id(), "metadata": metadata})
                    .to_string()
                    .into_bytes(),
                NO_DEPOSIT,
                OMNI_TOKEN_INIT_GAS,
            )
    }

    #[result_serializer(borsh)]
    pub fn get_token_code() -> Vec<u8> {
        BRIDGE_TOKEN_BINARY.to_vec()
    }

    pub fn get_token_code_hash() -> Base58CryptoHash {
        near_sdk::env::sha256_array(BRIDGE_TOKEN_BINARY).into()
    }
}
