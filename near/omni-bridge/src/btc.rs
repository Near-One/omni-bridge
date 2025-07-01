use crate::{
    ext_token, Contract, ContractExt, Role, FT_TRANSFER_CALL_GAS, FT_TRANSFER_GAS, MINT_TOKEN_GAS,
    ONE_YOCTO,
};
use bitcoin::{Address, Network, TxOut};
use near_plugins::{pause, AccessControllable, Pausable};
use near_sdk::json_types::U128;
use near_sdk::{
    env, near, require, serde_json, AccountId, Gas, NearToken, Promise, PromiseError,
    PromiseOrValue,
};
use omni_types::btc::TokenReceiverMessage;
use omni_types::near_events::OmniBridgeEvent;
use omni_types::{ChainKind, Fee, OmniAddress, TransferId, TransferMessage};
use std::str::FromStr;

const SUBMIT_TRANSFER_TO_BTC_CONNECTOR_CALLBACK_GAS: Gas = Gas::from_tgas(5);

#[near]
impl Contract {
    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn submit_transfer_to_btc_connector(
        &mut self,
        transfer_id: TransferId,
        msg: String,
        fee_recipient: Option<AccountId>,
        fee: &Option<Fee>,
    ) -> Promise {
        let transfer = self.get_transfer_message_storage(transfer_id);

        let message = serde_json::from_str::<TokenReceiverMessage>(&msg).expect("INVALID MSG");
        let amount = U128(transfer.message.amount.0 - transfer.message.fee.fee.0);

        if let OmniAddress::Btc(btc_address) = transfer.message.recipient.clone() {
            if let TokenReceiverMessage::Withdraw {
                target_btc_address,
                input: _,
                output,
            } = message
            {
                require!(
                    btc_address == target_btc_address,
                    "Incorrect target address"
                );
                let output_amount = self.get_output_amount(&output, &target_btc_address);

                let max_fee = transfer.message.msg.parse::<u64>();
                if let Ok(max_fee) = max_fee {
                    require!(
                        amount.0 - u128::from(output_amount) <= u128::from(max_fee),
                        "Fee exceeds max allowed fee"
                    );
                }
            } else {
                env::panic_str("Invalid message type");
            }
        } else {
            env::panic_str("Invalid destination chain");
        }

        if let Some(fee) = &fee {
            require!(&transfer.message.fee == fee, "Invalid fee");
        }

        require!(
            transfer.message.get_destination_chain() == ChainKind::Btc,
            "Incorrect destination chain"
        );
        let btc_account_id = self.get_native_token_id(ChainKind::Btc);
        require!(
            self.get_token_id(&transfer.message.token) == btc_account_id,
            "BTC account id"
        );

        self.remove_transfer_message(transfer_id);

        let fee_recipient = fee_recipient.unwrap_or(env::predecessor_account_id());

        ext_token::ext(btc_account_id)
            .with_attached_deposit(ONE_YOCTO)
            .with_static_gas(FT_TRANSFER_CALL_GAS)
            .ft_transfer_call(self.btc_connector.clone(), amount, None, msg)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(SUBMIT_TRANSFER_TO_BTC_CONNECTOR_CALLBACK_GAS)
                    .submit_transfer_to_btc_connector_callback(
                        transfer.message,
                        transfer.owner,
                        fee_recipient,
                    ),
            )
    }

    fn get_output_amount(&self, output: &[TxOut], target_address: &str) -> u64 {
        let Ok(target_address) = Address::from_str(target_address) else { env::panic_str("Invalid target address") };

        let network = self.get_btc_network();

        let Ok(checked_address) = target_address.require_network(network) else { env::panic_str("Invalid target address") };

        output
            .iter()
            .filter_map(|txout| {
                Address::from_script(&txout.script_pubkey, network)
                    .ok()
                    .filter(|addr| addr == &checked_address)
                    .map(|_| txout.value.to_sat())
            })
            .sum()
    }

    fn get_btc_network(&self) -> Network {
        if self.btc_connector.as_str().ends_with(".testnet") {
            Network::Testnet
        } else {
            Network::Bitcoin
        }
    }

    #[private]
    pub fn submit_transfer_to_btc_connector_callback(
        &mut self,
        transfer_msg: TransferMessage,
        transfer_owner: AccountId,
        fee_recipient: AccountId,
        #[callback_result] call_result: &Result<U128, PromiseError>,
    ) -> PromiseOrValue<()> {
        if matches!(call_result, Ok(result) if result.0 > 0) {
            if transfer_msg.fee.native_fee.0 != 0 {
                let origin_chain = transfer_msg.origin_transfer_id.map_or_else(
                    || transfer_msg.get_origin_chain(),
                    |origin_transfer_id| origin_transfer_id.origin_chain,
                );
                if origin_chain == ChainKind::Near {
                    Promise::new(fee_recipient.clone())
                        .transfer(NearToken::from_yoctonear(transfer_msg.fee.native_fee.0));
                } else {
                    ext_token::ext(self.get_native_token_id(origin_chain))
                        .with_static_gas(MINT_TOKEN_GAS)
                        .mint(fee_recipient.clone(), transfer_msg.fee.native_fee, None);
                }
            }

            let token = self.get_token_id(&transfer_msg.token);
            env::log_str(
                &OmniBridgeEvent::ClaimFeeEvent {
                    transfer_message: transfer_msg.clone(),
                }
                .to_log_string(),
            );

            let fee = transfer_msg.fee.fee;

            if fee.0 > 0 {
                PromiseOrValue::Promise(
                    ext_token::ext(token)
                        .with_static_gas(FT_TRANSFER_GAS)
                        .with_attached_deposit(ONE_YOCTO)
                        .ft_transfer(fee_recipient, fee, None),
                )
            } else {
                PromiseOrValue::Value(())
            }
        } else {
            self.insert_raw_transfer(transfer_msg, transfer_owner);
            PromiseOrValue::Value(())
        }
    }
}
