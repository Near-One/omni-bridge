use crate::storage::NEP141_DEPOSIT;
use crate::{
    ext_token, ext_utxo_connector, Contract, ContractExt, Role, FT_TRANSFER_CALL_GAS,
    FT_TRANSFER_GAS, ONE_YOCTO, STORAGE_DEPOSIT_GAS,
};
use near_plugins::{access_control_any, pause, AccessControllable, Pausable};
use near_sdk::json_types::U128;
use near_sdk::{
    env, near, require, serde_json, AccountId, Gas, NearToken, Promise, PromiseError,
    PromiseOrValue,
};
use omni_types::btc::{TokenReceiverMessage, TxOut, UTXOChainConfig};
use omni_types::errors::BridgeError;
use omni_types::{
    get_native_token_address, ChainKind, DestinationChainMsg, Fee, SubmitUtxoTransferMsg,
    TransferId, TransferMessage,
};
use omni_utils::macros::trusted_relayer;
use omni_utils::near_expect::NearExpect;
use omni_utils::promise::PromiseOrPromiseIndexOrValue;

/// Covers the callback itself plus the fee payment or the subsidy refund it schedules,
/// so a lack of gas fails the whole call before the transfer is submitted.
const SUBMIT_TRANSFER_TO_BTC_CONNECTOR_CALLBACK_GAS: Gas = Gas::from_tgas(20);
const WITHDRAW_RBF_GAS: Gas = Gas::from_tgas(100);

/// ZIP-317 fee parameters: <https://zips.z.cash/zip-0317>.
const ZIP317_MARGINAL_FEE: u128 = 5_000;
const ZIP317_GRACE_ACTIONS: u128 = 2;
/// The connector accepts a withdrawal Orchard bundle with exactly this many actions.
const ZCASH_ORCHARD_ACTIONS_PER_WITHDRAWAL: usize = 1;

/// ZIP-317 conventional fee of a Zcash transaction with the given transparent inputs and
/// outputs and shielded actions. This mirrors the connector's minimum fee: every input is
/// counted as a standard P2PKH one (one logical action each) and the transparent outputs by
/// their serialized size, which is one action per standard P2PKH/P2SH output.
fn zcash_zip317_fee(inputs_count: usize, outputs_count: usize, shielded_actions: usize) -> u128 {
    let logical_actions = (inputs_count.max(outputs_count) + shielded_actions) as u128;
    ZIP317_MARGINAL_FEE * logical_actions.max(ZIP317_GRACE_ACTIONS)
}

/// Checks that the relayer's gas subsidy tops up the user's `max_gas_fee` exactly to the gas fee
/// required for a UTXO chain transaction with the given number of inputs and outputs.
fn check_gas_subsidy(
    chain_kind: ChainKind,
    inputs_count: usize,
    outputs_count: usize,
    has_orchard_bundle: bool,
    user_max_gas_fee: u128,
    gas_subsidy: u128,
) {
    let required_gas_fee = match chain_kind {
        ChainKind::Zcash => {
            let shielded_actions = if has_orchard_bundle {
                ZCASH_ORCHARD_ACTIONS_PER_WITHDRAWAL
            } else {
                0
            };
            zcash_zip317_fee(inputs_count, outputs_count, shielded_actions)
        }
        // TODO: BTC fee is market-driven, so the subsidy can only be bounded from above
        // (e.g. by a DAO-configured max fee rate times the estimated vsize).
        ChainKind::Btc => env::panic_str(
            BridgeError::GasSubsidyNotImplementedForChain
                .to_string()
                .as_str(),
        ),
        _ => env::panic_str(
            BridgeError::GasSubsidyNotSupportedForChain
                .to_string()
                .as_str(),
        ),
    };

    require!(
        user_max_gas_fee < required_gas_fee,
        BridgeError::GasSubsidyNotNeeded.as_ref()
    );
    require!(
        user_max_gas_fee + gas_subsidy == required_gas_fee,
        BridgeError::InvalidGasSubsidy.as_ref()
    );
}

/// Gas fee top-up for a UTXO chain transaction paid by the relayer.
#[near(serializers=[json])]
#[derive(Debug, Clone)]
pub struct GasSubsidy {
    pub relayer_id: AccountId,
    pub amount: U128,
}

#[trusted_relayer]
#[near]
impl Contract {
    #[payable]
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn submit_transfer_to_utxo_chain_connector(
        &mut self,
        transfer_id: TransferId,
        msg: String,
        fee_recipient: Option<AccountId>,
        fee: &Option<Fee>,
    ) -> Promise {
        self.submit_transfer_to_utxo_chain_connector_internal(
            env::predecessor_account_id(),
            transfer_id,
            msg,
            fee_recipient,
            fee.as_ref(),
            None,
        )
    }

    /// Same as `submit_transfer_to_utxo_chain_connector`, but called via `ft_transfer_call`
    /// (manual mode for stuck transfers). The attached tokens are a relayer's subsidy for the
    /// UTXO chain gas fee. The subsidy is kept by the bridge right away, and the callback
    /// refunds it explicitly if the connector rejects the transfer. So if the callback fails,
    /// the subsidy stays in the bridge instead of being refunded from other users' funds.
    pub(crate) fn submit_transfer_to_utxo_chain_connector_with_gas_subsidy(
        &mut self,
        token_id: &AccountId,
        relayer_id: AccountId,
        gas_subsidy: U128,
        submit_utxo_transfer_msg: SubmitUtxoTransferMsg,
    ) -> PromiseOrPromiseIndexOrValue<U128> {
        // `relayer_id` is the `sender_id` of `ft_transfer_call`. It can be trusted, because
        // the internal method requires `token_id` to be the UTXO chain token.
        require!(
            self.is_trusted_relayer(&relayer_id),
            "Relayer is not active"
        );
        if !self.acl_has_any_role(vec![Role::DAO.into()], relayer_id.clone()) {
            require!(
                !self.pa_is_paused("submit_transfer_to_utxo_chain_connector".to_string()),
                "Pausable: Method is paused"
            );
        }

        self.submit_transfer_to_utxo_chain_connector_internal(
            relayer_id,
            submit_utxo_transfer_msg.transfer_id,
            submit_utxo_transfer_msg.msg,
            submit_utxo_transfer_msg.fee_recipient,
            submit_utxo_transfer_msg.fee.as_ref(),
            Some((token_id, gas_subsidy.0)),
        )
        .detach();

        PromiseOrPromiseIndexOrValue::Value(U128(0))
    }

    fn submit_transfer_to_utxo_chain_connector_internal(
        &mut self,
        relayer_id: AccountId,
        transfer_id: TransferId,
        msg: String,
        fee_recipient: Option<AccountId>,
        fee: Option<&Fee>,
        gas_subsidy: Option<(&AccountId, u128)>,
    ) -> Promise {
        let transfer = self.get_transfer_message_storage(transfer_id);

        let message = serde_json::from_str::<TokenReceiverMessage>(&msg).expect("INVALID MSG");
        let gas_subsidy_amount = gas_subsidy.map_or(0, |(_, amount)| amount);
        let amount =
            U128(transfer.message.amount.0 - transfer.message.fee.fee.0 + gas_subsidy_amount);
        let chain_kind = transfer.message.get_destination_chain();

        if let Some(btc_address) = transfer.message.recipient.get_utxo_address() {
            if let TokenReceiverMessage::Withdraw {
                target_btc_address,
                input,
                output,
                max_gas_fee,
                chain_specific_data,
            } = message
            {
                require!(
                    btc_address == target_btc_address,
                    BridgeError::IncorrectTargetUtxoAddress.as_ref()
                );

                let max_gas_fee_msg = DestinationChainMsg::from_json(&transfer.message.msg)
                    .and_then(|s| s.max_gas_fee());

                if gas_subsidy_amount > 0 {
                    let user_max_gas_fee =
                        max_gas_fee_msg.near_expect(BridgeError::InvalidMaxGasFee);
                    check_gas_subsidy(
                        chain_kind,
                        input.len(),
                        output.len(),
                        chain_specific_data.is_some(),
                        user_max_gas_fee.0,
                        gas_subsidy_amount,
                    );
                    require!(
                        max_gas_fee == Some(U128(user_max_gas_fee.0 + gas_subsidy_amount)),
                        BridgeError::InvalidMaxGasFee.as_ref()
                    );
                } else if let Some(max_gas_fee_msg) = max_gas_fee_msg {
                    require!(
                        max_gas_fee.expect("max_gas_fee is missing") == max_gas_fee_msg,
                        "Invalid max gas fee"
                    );
                }
            } else {
                env::panic_str("Invalid message type");
            }
        } else {
            env::panic_str("Invalid destination chain");
        }

        if let Some(fee) = fee {
            require!(
                &transfer.message.fee == fee,
                BridgeError::InvalidFee.as_ref()
            );
        }

        let btc_account_id = self.get_utxo_chain_token(chain_kind);
        require!(
            self.get_token_id(&transfer.message.token) == btc_account_id,
            BridgeError::NativeTokenRequiredForChain.as_ref()
        );
        if let Some((gas_subsidy_token_id, _)) = gas_subsidy {
            require!(
                gas_subsidy_token_id == &btc_account_id,
                BridgeError::InvalidGasSubsidyToken.as_ref()
            );
        }

        self.remove_transfer_message(transfer_id);

        let fee_recipient = fee_recipient.unwrap_or_else(|| relayer_id.clone());
        let gas_subsidy = (gas_subsidy_amount > 0).then_some(GasSubsidy {
            relayer_id,
            amount: U128(gas_subsidy_amount),
        });

        ext_token::ext(btc_account_id)
            .with_attached_deposit(ONE_YOCTO)
            .with_static_gas(FT_TRANSFER_CALL_GAS)
            .ft_transfer_call(self.get_utxo_chain_connector(chain_kind), amount, None, msg)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(SUBMIT_TRANSFER_TO_BTC_CONNECTOR_CALLBACK_GAS)
                    .submit_transfer_to_btc_connector_callback(
                        transfer.message,
                        transfer.owner,
                        fee_recipient,
                        gas_subsidy,
                    ),
            )
    }

    #[private]
    pub fn submit_transfer_to_btc_connector_callback(
        &mut self,
        transfer_msg: TransferMessage,
        transfer_owner: AccountId,
        fee_recipient: AccountId,
        // `Option` keeps callbacks scheduled before the upgrade (without this argument) working.
        gas_subsidy: Option<GasSubsidy>,
        #[callback_result] call_result: &Result<U128, PromiseError>,
    ) -> PromiseOrValue<()> {
        if matches!(call_result, Ok(result) if result.0 > 0) {
            let token_fee = transfer_msg.fee.fee.0;
            self.send_fee_internal(&transfer_msg, fee_recipient, token_fee)
        } else {
            let gas_subsidy_refund = gas_subsidy
                .map(|gas_subsidy| (self.get_token_id(&transfer_msg.token), gas_subsidy));
            let required_storage_balance =
                self.add_transfer_message(transfer_msg, transfer_owner.clone());

            self.update_storage_balance(
                transfer_owner,
                required_storage_balance,
                NearToken::from_yoctonear(0),
            );

            if let Some((token_id, gas_subsidy)) = gas_subsidy_refund {
                ext_token::ext(token_id)
                    .with_static_gas(FT_TRANSFER_GAS)
                    .with_attached_deposit(ONE_YOCTO)
                    .ft_transfer(gas_subsidy.relayer_id, gas_subsidy.amount, None)
                    .into()
            } else {
                PromiseOrValue::Value(())
            }
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn add_utxo_chain_connector(
        &mut self,
        chain_kind: ChainKind,
        utxo_chain_connector_id: AccountId,
        utxo_chain_token_id: AccountId,
        decimals: u8,
    ) {
        let storage_usage = env::storage_usage();
        let token_address = get_native_token_address(chain_kind)
            .near_expect(BridgeError::FailedToGetNativeTokenAddress);

        let chain_kind = token_address.get_chain();
        self.token_id_to_address
            .insert(&(chain_kind, utxo_chain_token_id.clone()), &token_address);
        self.token_address_to_id
            .insert(&token_address, &utxo_chain_token_id);
        self.token_decimals.insert(
            &token_address,
            &crate::storage::Decimals {
                decimals,
                origin_decimals: decimals,
            },
        );

        self.utxo_chain_connectors.insert(
            chain_kind,
            UTXOChainConfig {
                connector: utxo_chain_connector_id,
                token_id: utxo_chain_token_id.clone(),
            },
        );

        let required_deposit = NEP141_DEPOSIT.saturating_add(
            env::storage_byte_cost()
                .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into()),
        );

        require!(
            env::attached_deposit() >= required_deposit,
            BridgeError::InsufficientStorageDeposit.as_ref()
        );

        ext_token::ext(utxo_chain_token_id)
            .with_static_gas(STORAGE_DEPOSIT_GAS)
            .with_attached_deposit(NEP141_DEPOSIT)
            .storage_deposit(&env::current_account_id(), Some(true))
            .detach();
    }

    #[access_control_any(roles(Role::DAO, Role::RbfOperator))]
    pub fn rbf_increase_gas_fee(
        &self,
        chain_kind: ChainKind,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) -> Promise {
        ext_utxo_connector::ext(self.get_utxo_chain_connector(chain_kind))
            .with_static_gas(WITHDRAW_RBF_GAS)
            .withdraw_rbf(original_btc_pending_verify_id, output)
    }

    /// Returns the `AccountId` of the connector for the given UTXO chain.
    ///
    /// # Panics
    ///
    /// Panics if a Сonnector for the specified `chain_kind` has not been configured.
    pub fn get_utxo_chain_connector(&self, chain_kind: ChainKind) -> AccountId {
        self.utxo_chain_connectors
            .get(&chain_kind)
            .expect("Connector has not been set up for this chain")
            .connector
            .clone()
    }

    /// Returns the `AccountId` of the token for the given UTXO chain.
    ///
    /// # Panics
    ///
    /// Panics if a UTXO chain Token for the specified `chain_kind` has not been configured.
    pub fn get_utxo_chain_token(&self, chain_kind: ChainKind) -> AccountId {
        self.utxo_chain_connectors
            .get(&chain_kind)
            .expect("UTXO Token has not been set up for this chain")
            .token_id
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zcash_zip317_fee_uses_grace_actions_for_small_transactions() {
        assert_eq!(zcash_zip317_fee(1, 1, 0), 10_000);
        assert_eq!(zcash_zip317_fee(2, 2, 0), 10_000);
        assert_eq!(zcash_zip317_fee(1, 0, 1), 10_000);
    }

    #[test]
    fn zcash_zip317_fee_uses_max_of_inputs_and_outputs() {
        assert_eq!(zcash_zip317_fee(4, 2, 0), 20_000);
        assert_eq!(zcash_zip317_fee(2, 3, 0), 15_000);
    }

    #[test]
    fn zcash_zip317_fee_adds_shielded_actions() {
        assert_eq!(zcash_zip317_fee(4, 1, 1), 25_000);
        assert_eq!(zcash_zip317_fee(2, 0, 1), 15_000);
    }

    #[test]
    fn check_gas_subsidy_exact_top_up_succeeds() {
        check_gas_subsidy(ChainKind::Zcash, 4, 2, false, 15_000, 5_000);
    }

    #[test]
    fn check_gas_subsidy_orchard_withdrawal_counts_extra_action() {
        check_gas_subsidy(ChainKind::Zcash, 4, 1, true, 15_000, 10_000);
    }

    #[test]
    #[should_panic(expected = "ERR_INVALID_GAS_SUBSIDY")]
    fn check_gas_subsidy_transparent_subsidy_for_orchard_withdrawal_fails() {
        check_gas_subsidy(ChainKind::Zcash, 4, 1, true, 15_000, 5_000);
    }

    #[test]
    #[should_panic(expected = "ERR_INVALID_GAS_SUBSIDY")]
    fn check_gas_subsidy_excessive_subsidy_fails() {
        check_gas_subsidy(ChainKind::Zcash, 4, 2, false, 15_000, 50_000);
    }

    #[test]
    #[should_panic(expected = "ERR_INVALID_GAS_SUBSIDY")]
    fn check_gas_subsidy_insufficient_subsidy_fails() {
        check_gas_subsidy(ChainKind::Zcash, 4, 2, false, 15_000, 4_999);
    }

    #[test]
    #[should_panic(expected = "ERR_GAS_SUBSIDY_NOT_NEEDED")]
    fn check_gas_subsidy_user_max_gas_fee_is_enough_fails() {
        check_gas_subsidy(ChainKind::Zcash, 4, 2, false, 20_000, 1);
    }

    #[test]
    #[should_panic(expected = "ERR_GAS_SUBSIDY_NOT_IMPLEMENTED_FOR_CHAIN")]
    fn check_gas_subsidy_btc_fails() {
        check_gas_subsidy(ChainKind::Btc, 4, 2, false, 15_000, 5_000);
    }

    #[test]
    #[should_panic(expected = "ERR_GAS_SUBSIDY_NOT_SUPPORTED_FOR_CHAIN")]
    fn check_gas_subsidy_non_utxo_chain_fails() {
        check_gas_subsidy(ChainKind::Eth, 4, 2, false, 15_000, 5_000);
    }
}
