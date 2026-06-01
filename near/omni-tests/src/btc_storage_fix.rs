#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::types::NearToken;
    use omni_types::{
        btc::TokenReceiverMessage, BridgeOnTransferMsg, ChainKind, Fee, InitTransferMsg,
        OmniAddress, TransferId,
    };
    use rstest::rstest;

    use crate::{
        environment::TestEnvBuilder,
        helpers::tests::{account_n, build_artifacts, BuildArtifacts},
    };

    fn btc_recipient_address() -> OmniAddress {
        "btc:bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
            .parse()
            .unwrap()
    }

    async fn get_available_balance(
        bridge_contract: &near_workspaces::Contract,
        account_id: &AccountId,
    ) -> anyhow::Result<NearToken> {
        let result: serde_json::Value = bridge_contract
            .view("storage_balance_of")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?;

        let available_str = result["available"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'available' field in storage_balance_of"))?;

        Ok(NearToken::from_yoctonear(available_str.parse::<u128>()?))
    }

    /// Verify that storage balance is preserved when `submit_transfer_to_utxo_chain_connector`
    /// fails and the callback re-inserts the transfer.
    #[rstest]
    #[tokio::test]
    async fn test_btc_callback_failure_preserves_storage_balance(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env_builder = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_utxo_token()
            .await?;

        let sender_account = env_builder.create_account(account_n(1)).await?;
        let relayer_account = env_builder
            .setup_trusted_relayer("relayer".parse().unwrap())
            .await?;

        env_builder.storage_deposit(sender_account.id()).await?;

        let required_balance_account: NearToken = env_builder
            .bridge_contract
            .view("required_balance_for_account")
            .await?
            .json()?;

        sender_account
            .call(env_builder.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": sender_account.id() }))
            .deposit(required_balance_account.saturating_add(NearToken::from_near(1)))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        relayer_account
            .call(env_builder.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": relayer_account.id() }))
            .deposit(NearToken::from_near(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        env_builder.storage_deposit(relayer_account.id()).await?;

        let transfer_amount: u128 = 100_000_000;
        env_builder
            .mint_tokens(sender_account.id(), transfer_amount)
            .await?;

        env_builder
            .bridge_contract
            .call("set_locked_tokens")
            .args_json(json!({
                "args": [{
                    "chain_kind": ChainKind::Btc,
                    "token_id": env_builder.token.contract.id(),
                    "amount": U128(0),
                }]
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let init_transfer_msg = InitTransferMsg {
            recipient: btc_recipient_address(),
            fee: U128(1000),
            native_token_fee: U128(0),
            msg: None,
            external_id: None,
        };

        let transfer_result = sender_account
            .call(env_builder.token.contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": env_builder.bridge_contract.id(),
                "amount": U128(transfer_amount),
                "memo": None::<String>,
                "msg": serde_json::to_string(
                    &BridgeOnTransferMsg::InitTransfer(init_transfer_msg),
                )?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?;

        assert!(
            transfer_result.failures().is_empty(),
            "Init transfer should succeed: {:?}",
            transfer_result.failures()
        );

        let transfer_id = TransferId {
            origin_chain: ChainKind::Near,
            origin_nonce: 1,
        };

        let transfer_message: serde_json::Value = env_builder
            .bridge_contract
            .view("get_transfer_message")
            .args_json(json!({ "transfer_id": transfer_id }))
            .await?
            .json()?;
        assert!(
            !transfer_message.is_null(),
            "Transfer should exist after init"
        );

        let available_before =
            get_available_balance(&env_builder.bridge_contract, sender_account.id()).await?;

        let btc_address = btc_recipient_address()
            .get_utxo_address()
            .expect("should be a BTC address");

        let withdraw_msg = TokenReceiverMessage::Withdraw {
            target_btc_address: btc_address,
            input: vec![],
            output: vec![],
            max_gas_fee: None,
        };

        // The mock connector doesn't implement ft_on_transfer, so the call fails
        // and the callback re-inserts the transfer.
        let _submit_result = relayer_account
            .call(
                env_builder.bridge_contract.id(),
                "submit_transfer_to_utxo_chain_connector",
            )
            .args_json(json!({
                "transfer_id": transfer_id,
                "msg": serde_json::to_string(&withdraw_msg)?,
                "fee_recipient": None::<AccountId>,
                "fee": Fee { fee: U128(1000), native_fee: U128(0) },
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?;

        let transfer_after: serde_json::Value = env_builder
            .bridge_contract
            .view("get_transfer_message")
            .args_json(json!({ "transfer_id": transfer_id }))
            .await?
            .json()?;
        assert!(
            !transfer_after.is_null(),
            "Transfer should be re-inserted after failed connector call"
        );

        let available_after =
            get_available_balance(&env_builder.bridge_contract, sender_account.id()).await?;

        assert_eq!(
            available_before,
            available_after,
            "Storage balance should be unchanged after failed submit: \
             remove_transfer refunds and add_transfer deducts the same amount. \
             Before={}, After={} (diff={} yoctoNEAR)",
            available_before.as_yoctonear(),
            available_after.as_yoctonear(),
            available_after
                .as_yoctonear()
                .saturating_sub(available_before.as_yoctonear()),
        );

        Ok(())
    }
}
