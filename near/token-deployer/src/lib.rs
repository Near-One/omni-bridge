use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};

const BRIDGE_TOKEN_INIT_BALANCE: NearToken = NearToken::from_near(3);
const NO_DEPOSIT: NearToken = NearToken::from_near(0);
const OMNI_TOKEN_INIT_GAS: Gas = Gas::from_tgas(10);

const BRIDGE_TOKEN_BINARY: &'static [u8] =
    include_bytes!("../.././target/wasm32-unknown-unknown/release/omni_token.wasm");

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    pub controller: AccountId,
}

#[near]
impl Contract {
    #[init]
    pub fn new(controller: AccountId) -> Self {
        Self { controller }
    }

    pub fn deploy_token(&mut self, account_id: AccountId, init_args: Vec<u8>) -> Promise {
        assert_eq!(
            env::predecessor_account_id(),
            self.controller,
            "ERR_NOT_OWNER"
        );

        Promise::new(account_id)
            .create_account()
            .transfer(BRIDGE_TOKEN_INIT_BALANCE)
            .deploy_contract(BRIDGE_TOKEN_BINARY.to_vec())
            .function_call(
                "new".to_string(),
                init_args,
                NO_DEPOSIT,
                OMNI_TOKEN_INIT_GAS,
            )
    }
}