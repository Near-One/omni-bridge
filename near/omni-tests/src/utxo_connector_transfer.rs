#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{result::ExecutionFinalResult, types::NearToken};
    use omni_types::{
        BridgeOnTransferMsg, ChainKind, InitTransferMsg, OmniAddress, TransferId, TransferMessage,
    };
    use rstest::rstest;

    use crate::helpers::tests::{account_n, base_eoa_address, build_artifacts};
    use crate::{environment::*, helpers::tests::BuildArtifacts};

    struct TestEnv {
        token_contract: near_workspaces::Contract,
        bridge_contract: near_workspaces::Contract,
        utxo_connector: near_workspaces::Contract,
        relayer_account: near_workspaces::Account,
    }

    impl TestEnv {
        async fn new(build_artifacts: &BuildArtifacts) -> anyhow::Result<Self> {
            let env_builder = TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .with_utxo_token()
                .await?;

            let relayer_account = env_builder.setup_trusted_relayer(account_n(10)).await?;
            env_builder.storage_deposit(relayer_account.id()).await?;
            env_builder
                .omni_storage_deposit(relayer_account.id(), 1_000_000_000_000_000_000_000_000)
                .await?;
            env_builder
                .mint_tokens(relayer_account.id(), 1_000_000_000)
                .await?;
            env_builder
                .bridge_contract
                .call("set_locked_tokens")
                .args_json(json!({
                    "args": [
                        {
                            "chain_kind": ChainKind::Base,
                            "token_id": env_builder.token.contract.id(),
                            "amount": U128(0),
                        },
                        {
                            "chain_kind": ChainKind::Near,
                            "token_id": env_builder.token.contract.id(),
                            "amount": U128(1_000_000_000),
                        }
                    ]
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            env_builder
                .mint_tokens(
                    env_builder.utxo_connector.as_ref().unwrap().id(),
                    1_000_000_000_000,
                )
                .await?;

            Ok(Self {
                token_contract: env_builder.token.contract,
                bridge_contract: env_builder.bridge_contract,
                utxo_connector: env_builder.utxo_connector.unwrap(),
                relayer_account,
            })
        }
    }

    async fn get_balance(
        token_contract: &near_workspaces::Contract,
        account_id: &AccountId,
    ) -> anyhow::Result<U128> {
        let balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": account_id,
            }))
            .await?
            .json()?;

        Ok(balance)
    }

    fn has_error_message(result: &ExecutionFinalResult, error_msg: &str) -> bool {
        let has_failure = result.failures().into_iter().any(|outcome| {
            outcome
                .clone()
                .into_result()
                .is_err_and(|err| format!("{err:?}").contains(error_msg))
        });

        has_failure || result.logs().iter().any(|log| log.contains(error_msg))
    }

    fn init_transfer_msg(native_token_fee: u128) -> String {
        serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(InitTransferMsg {
            recipient: base_eoa_address(),
            fee: U128(1000),
            native_token_fee: U128(native_token_fee),
            msg: None,
            external_id: None,
        }))
        .unwrap()
    }

    async fn get_transfer_message(
        bridge_contract: &near_workspaces::Contract,
    ) -> Option<TransferMessage> {
        bridge_contract
            .view("get_transfer_message")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: 1,
                },
            }))
            .await
            .ok()
            .and_then(|r| r.json().ok())
    }

    #[rstest]
    #[tokio::test]
    async fn connector_init_transfer_without_native_fee_succeeds(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(build_artifacts).await?;
        let amount: u128 = 100_000_000;

        let connector_balance_before =
            get_balance(&env.token_contract, env.utxo_connector.id()).await?;

        let result = env
            .relayer_account
            .call(env.utxo_connector.id(), "verify_deposit")
            .args_json(json!({
                "amount": U128(amount),
                "msg": init_transfer_msg(0),
            }))
            .max_gas()
            .transact()
            .await?;

        assert!(
            result.failures().is_empty(),
            "Unexpected failures: {:?}",
            result.failures()
        );

        let connector_balance_after =
            get_balance(&env.token_contract, env.utxo_connector.id()).await?;
        assert_eq!(
            connector_balance_before.0,
            connector_balance_after.0 + amount,
            "Connector should have spent the deposited amount"
        );

        let transfer_message = get_transfer_message(&env.bridge_contract)
            .await
            .expect("Transfer message should be created");
        assert_eq!(transfer_message.amount.0, amount);
        assert_eq!(transfer_message.recipient, base_eoa_address());
        assert_eq!(
            transfer_message.sender,
            OmniAddress::Near(env.utxo_connector.id().clone())
        );
        assert_eq!(transfer_message.fee.native_fee.0, 0);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn connector_init_transfer_with_native_fee_fails(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(build_artifacts).await?;
        let amount: u128 = 100_000_000;

        let connector_balance_before =
            get_balance(&env.token_contract, env.utxo_connector.id()).await?;
        let relayer_storage_before: serde_json::Value = env
            .bridge_contract
            .view("storage_balance_of")
            .args_json(json!({ "account_id": env.relayer_account.id() }))
            .await?
            .json()?;

        let result = env
            .relayer_account
            .call(env.utxo_connector.id(), "verify_deposit")
            .args_json(json!({
                "amount": U128(amount),
                "msg": init_transfer_msg(1_000_000),
            }))
            .max_gas()
            .transact()
            .await?;

        assert!(has_error_message(&result, "ERR_NATIVE_FEE_FOR_UTXO_CHAIN"));

        let connector_balance_after =
            get_balance(&env.token_contract, env.utxo_connector.id()).await?;
        assert_eq!(
            connector_balance_before.0, connector_balance_after.0,
            "Tokens should be fully refunded to the connector"
        );

        let relayer_storage_after: serde_json::Value = env
            .bridge_contract
            .view("storage_balance_of")
            .args_json(json!({ "account_id": env.relayer_account.id() }))
            .await?
            .json()?;
        assert_eq!(
            relayer_storage_before["available"], relayer_storage_after["available"],
            "Relayer storage balance must not be charged"
        );

        assert!(get_transfer_message(&env.bridge_contract).await.is_none());

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_untrusted_account_cannot_call_submit_transfer_to_utxo_chain_connector(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(build_artifacts).await?;

        // Create a random account that has no roles
        let random_account = env
            .bridge_contract
            .as_account()
            .create_subaccount("random-user")
            .initial_balance(NearToken::from_near(5))
            .transact()
            .await?
            .into_result()?;

        // Verify this account is not a trusted relayer
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": random_account.id()}))
            .await?
            .json()?;
        assert!(
            !is_trusted,
            "random account should not be a trusted relayer"
        );

        // Attempt to call submit_transfer_to_utxo_chain_connector from the random account.
        // The #[trusted_relayer] guard should reject this before the method body runs,
        // so a dummy transfer_id is sufficient.
        let result = random_account
            .call(
                env.bridge_contract.id(),
                "submit_transfer_to_utxo_chain_connector",
            )
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: 1,
                },
                "msg": "{}",
                "fee_recipient": null,
                "fee": null,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?;

        assert!(
            result.into_result().is_err(),
            "Unprivileged account should not be able to call submit_transfer_to_utxo_chain_connector"
        );

        Ok(())
    }
}
