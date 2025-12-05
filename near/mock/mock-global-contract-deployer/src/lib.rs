use near_sdk::json_types::Base64VecU8;
use near_sdk::{env, near, AccountId, Promise};

#[near(contract_state)]
#[derive(Default)]
pub struct Contract {}

#[near]
impl Contract {
    /// Deploy a global contract with the given bytecode, identifiable by its code hash
    #[payable]
    pub fn deploy_global_contract(&mut self, code: Base64VecU8, account_id: AccountId) -> Promise {
        let code_bytes: Vec<u8> = code.into();

        Promise::new(account_id)
            .create_account()
            .transfer(env::attached_deposit())
            .add_full_access_key(env::signer_account_pk())
            .deploy_global_contract(code_bytes)
    }

    /// Deploy a global contract, identifiable by the predecessor's account ID
    #[payable]
    pub fn deploy_global_contract_by_account_id(
        &mut self,
        code: Base64VecU8,
        account_id: AccountId,
    ) -> Promise {
        let code_bytes: Vec<u8> = code.into();

        Promise::new(account_id)
            .create_account()
            .transfer(env::attached_deposit())
            .add_full_access_key(env::signer_account_pk())
            .deploy_global_contract_by_account_id(code_bytes)
    }
}
