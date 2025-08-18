use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use near_sdk::json_types::U128;
use near_sdk::{env, near, serde_json, AccountId, PromiseOrValue};

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
        env::log_str(&format!(
            "ft_on_transfer called with sender_id: {}, amount: {}, msg: {}",
            sender_id, amount.0, msg
        ));

        let msg = serde_json::from_str::<TestMessage>(&msg).unwrap();

        if msg.panic {
            env::panic_str("Panic from mock-token-receiver");
        }

        PromiseOrValue::Value(msg.return_value)
    }
}

#[near(serializers=[json])]
struct TestMessage {
    return_value: U128,
    panic: bool,
    extra_msg: String,
}
