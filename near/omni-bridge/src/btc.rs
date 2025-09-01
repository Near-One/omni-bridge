use crate::{ext_token, Contract, ContractExt, Role, FT_TRANSFER_CALL_GAS, ONE_YOCTO};
use near_plugins::{access_control_any, pause, AccessControllable, Pausable};
use near_sdk::json_types::U128;
use near_sdk::{
    env, near, require, serde_json, AccountId, Gas, Promise, PromiseError, PromiseOrValue,
};
use omni_types::btc::{TokenReceiverMessage, UTXOChainConfig};
use omni_types::{ChainKind, Fee, OmniAddress, TransferId, TransferMessage};

const SUBMIT_TRANSFER_TO_BTC_CONNECTOR_CALLBACK_GAS: Gas = Gas::from_tgas(5);
#[near]
impl Contract {
    #[payable]
    #[pause(except(roles(Role::DAO, Role::UnrestrictedRelayer)))]
    pub fn submit_transfer_to_utxo_chain_connector(
        &mut self,
        transfer_id: TransferId,
        msg: String,
        fee_recipient: Option<AccountId>,
        fee: &Option<Fee>,
    ) -> Promise {
        let transfer = self.get_transfer_message_storage(transfer_id);

        let message = serde_json::from_str::<TokenReceiverMessage>(&msg).expect("INVALID MSG");
        let amount = U128(transfer.message.amount.0 - transfer.message.fee.fee.0);

        if let Some(btc_address) = transfer.message.recipient.get_utxo_address() {
            if let TokenReceiverMessage::Withdraw {
                target_btc_address,
                input: _,
                output: _,
                max_gas_fee,
            } = message
            {
                require!(
                    btc_address == target_btc_address,
                    "Incorrect target address"
                );

                let max_fee = transfer.message.msg.parse::<u64>();
                if let Ok(max_fee) = max_fee {
                    require!(
                        max_gas_fee.expect("max_gas_fee is missing").0 == max_fee.into(),
                        "Invalid max fee"
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

        let chain_kind = transfer.message.get_destination_chain();
        let btc_account_id = self.get_utxo_chain_token(chain_kind);
        require!(
            self.get_token_id(&transfer.message.token) == btc_account_id,
            "Only the native token of this UTXO chain can be transferred."
        );

        self.remove_transfer_message(transfer_id);

        let fee_recipient = fee_recipient.unwrap_or(env::predecessor_account_id());

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
                    ),
            )
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
            let token_fee = transfer_msg.fee.fee.0;
            self.send_fee_internal(&transfer_msg, fee_recipient, token_fee)
        } else {
            self.insert_raw_transfer(transfer_msg, transfer_owner);
            PromiseOrValue::Value(())
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
        let token_address = OmniAddress::new_zero(chain_kind)
            .unwrap_or_else(|_| env::panic_str("ERR_FAILED_TO_GET_ZERO_ADDRESS"));

        self.add_token(&utxo_chain_token_id, &token_address, decimals, decimals);

        self.utxo_chain_connectors.insert(
            &chain_kind,
            &UTXOChainConfig {
                connector: utxo_chain_connector_id,
                token_id: utxo_chain_token_id,
            },
        );

        let required_deposit = env::storage_byte_cost()
            .saturating_mul((env::storage_usage().saturating_sub(storage_usage)).into());

        require!(
            env::attached_deposit() >= required_deposit,
            "ERROR: The deposit is not sufficient to cover the storage."
        );
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
    }
}
