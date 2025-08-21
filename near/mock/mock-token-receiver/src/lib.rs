use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use near_sdk::json_types::U128;
use near_sdk::{near, AccountId, PromiseOrValue};

#[near(contract_state)]
#[derive(Default)]
pub struct Contract {}

#[near]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        PromiseOrValue::Value(U128(0))
    }
}
