use near_plugins::{
    access_control, access_control_any, AccessControlRole, AccessControllable, Pausable, Upgradable,
};
use near_sdk::borsh::BorshDeserialize;
use near_sdk::serde_json::json;
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};
use omni_types::BasicMetadata;

const NO_DEPOSIT: NearToken = NearToken::from_near(0);
const OMNI_TOKEN_INIT_GAS: Gas = Gas::from_tgas(10);

#[near(serializers = [json])]
#[derive(AccessControlRole, Copy, Clone)]
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
pub struct TokenDeployer {
    omni_token_global_contract_id: AccountId,
}

#[near]
impl TokenDeployer {
    #[init]
    pub fn new(
        controller: AccountId,
        dao: AccountId,
        omni_token_global_contract_id: AccountId,
    ) -> Self {
        let mut contract = Self {
            omni_token_global_contract_id,
        };

        contract.acl_init_super_admin(near_sdk::env::predecessor_account_id());
        contract.acl_grant_role(Role::DAO.into(), dao.clone());
        contract.acl_grant_role(Role::Controller.into(), controller);
        contract.acl_transfer_super_admin(dao);
        contract
    }

    #[access_control_any(roles(Role::Controller))]
    pub fn deploy_token(&mut self, account_id: AccountId, metadata: &BasicMetadata) -> Promise {
        Promise::new(account_id)
            .create_account()
            .use_global_contract_by_account_id(self.omni_token_global_contract_id.clone())
            .function_call(
                "new".to_string(),
                json!({"controller": env::predecessor_account_id(), "metadata": metadata})
                    .to_string()
                    .into_bytes(),
                NO_DEPOSIT,
                OMNI_TOKEN_INIT_GAS,
            )
    }

    pub fn get_omni_token_global_contract_id(&self) -> AccountId {
        self.omni_token_global_contract_id.clone()
    }

    #[access_control_any(roles(Role::DAO))]
    pub fn set_omni_token_global_contract_id(&mut self, omni_token_global_contract_id: AccountId) {
        self.omni_token_global_contract_id = omni_token_global_contract_id;
    }
}
