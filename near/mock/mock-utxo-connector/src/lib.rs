use near_sdk::json_types::U128;
use near_sdk::serde_json;
use near_sdk::{ext_contract, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};
use omni_types::{BridgeOnTransferMsg, UtxoFinTransferMsg};

const FT_TRANSFER_CALL_GAS: Gas = Gas::from_tgas(210);
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

#[near]
impl MockUtxoConnector {
    #[init]
    pub fn new(bridge_account: AccountId, token_account: AccountId) -> Self {
        Self {
            bridge_account,
            token_account,
        }
    }

    pub fn verify_deposit(&mut self, amount: U128, msg: UtxoFinTransferMsg) -> Promise {
        ext_token::ext(self.token_account.clone())
            .with_attached_deposit(ONE_YOCTO)
            .with_static_gas(FT_TRANSFER_CALL_GAS)
            .ft_transfer_call(
                self.bridge_account.clone(),
                amount,
                None,
                serde_json::to_string(&BridgeOnTransferMsg::UtxoFinTransfer(msg)).unwrap(),
            )
    }

    /// NEP-141 ft_on_transfer implementation
    /// This is called when tokens are transferred to this connector
    /// Returns the amount to refund (0 means accept all tokens)
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128 {
        near_sdk::env::log_str(&format!(
            "MockUtxoConnector received {} tokens from {} with msg: {}",
            amount.0, sender_id, msg
        ));

        // Accept all tokens (no refund)
        U128(0)
    }
}
