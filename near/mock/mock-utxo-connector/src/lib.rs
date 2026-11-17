use near_sdk::json_types::U128;
use near_sdk::{ext_contract, near, AccountId, NearToken, PanicOnDefault, Promise};

const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

#[ext_contract(ext_token)]
pub trait ExtToken {
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    );
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct MockUtxoConnector {
    pub bridge_account: AccountId,
    pub token_account: AccountId,
}

#[allow(clippy::needless_pass_by_value)]
#[near]
impl MockUtxoConnector {
    #[init]
    pub fn new(bridge_account: AccountId, token_account: AccountId) -> Self {
        Self {
            bridge_account,
            token_account,
        }
    }

    /// Mimics the real connector's safe deposit: forwards the depositor-supplied `msg`
    /// to the bridge verbatim via `ft_transfer_call`.
    pub fn verify_deposit(&mut self, amount: U128, msg: String) -> Promise {
        ext_token::ext(self.token_account.clone())
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer_call(self.bridge_account.clone(), amount, None, msg)
    }
}
