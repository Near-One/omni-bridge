use near_sdk::json_types::U128;
use near_sdk::{env, serde_json, Gas, PromiseOrValue};
use near_sdk::{ext_contract, near, AccountId, NearToken, PanicOnDefault, Promise};
use omni_types::{BridgeOnTransferMsg, UtxoFinTransferMsg};

const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const SIGNBTC_TRANSACTION_GAS: Gas = Gas::from_tgas(25);

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

    #[allow(clippy::missing_panics_doc)]
    pub fn verify_deposit(&mut self, amount: U128, msg: UtxoFinTransferMsg) -> Promise {
        ext_token::ext(self.token_account.clone())
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer_call(
                self.bridge_account.clone(),
                amount,
                None,
                serde_json::to_string(&BridgeOnTransferMsg::UtxoFinTransfer(msg)).unwrap(),
            )
    }

    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        env::log_str(&format!(
            "Mock ft_on_transfer called with sender_id: {}, amount: {}, msg: {}, gas: {}",
            sender_id,
            amount.0,
            msg,
            env::prepaid_gas()
        ));
        PromiseOrValue::Value(U128(0))
    }

    pub fn sign_btc_transaction(
        &self,
        btc_pending_sign_id: String,
        sign_index: usize,
        key_version: u32,
    ) -> PromiseOrValue<bool> {
        env::log_str(&format!(
            "Mock sign_btc_transaction called with btc_pending_sign_id: {}, sign_index: {}, key_version: {}, gas: {}",
            btc_pending_sign_id, sign_index, key_version, env::prepaid_gas()
        ));

        Self::ext(env::current_account_id())
            .with_static_gas(SIGNBTC_TRANSACTION_GAS)
            .self_call()
            .into()
    }

    pub fn self_call(&self) {
        env::log_str(&format!(
            "Mock self_call called with gas: {}",
            env::prepaid_gas()
        ));
    }
}
