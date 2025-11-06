#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{result::ExecutionFinalResult, types::NearToken};
    use omni_types::{
        BridgeOnTransferMsg, ChainKind, ChainTransferId, FastFinTransferMsg, Fee, OmniAddress,
        UnifiedTransferId, UtxoFinTransferMsg,
    };
    use rstest::rstest;

    use crate::helpers::tests::{account_n, base_eoa_address, build_artifacts};
    use crate::{environment::*, helpers::tests::BuildArtifacts};

    struct TestEnv {
        token_contract: near_workspaces::Contract,
        bridge_contract: near_workspaces::Contract,
        utxo_connector: near_workspaces::Contract,
        relayer_account: near_workspaces::Account,
        recipient_account: near_workspaces::Account,
    }

    impl TestEnv {
        async fn new(build_artifacts: &BuildArtifacts) -> anyhow::Result<Self> {
            let env_builder = TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .with_utxo_token()
                .await?;

            let relayer_account = env_builder.create_account(account_n(10)).await?;
            env_builder.storage_deposit(relayer_account.id()).await?;
            env_builder
                .omni_storage_deposit(relayer_account.id(), 1_000_000_000_000_000_000_000_000)
                .await?;
            env_builder
                .mint_tokens(relayer_account.id(), 1_000_000_000)
                .await?;

            let recipient_account = env_builder.create_account(account_n(1)).await?;
            env_builder.storage_deposit(recipient_account.id()).await?;

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
                recipient_account,
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
        result.failures().into_iter().any(|outcome| {
            outcome
                .clone()
                .into_result()
                .is_err_and(|err| format!("{err:?}").contains(error_msg))
        })
    }

    fn default_utxo_fin_transfer() -> UtxoFinTransferMsg {
        UtxoFinTransferMsg {
            utxo_id: "btc:abc94fc5b954136a691594c7044bcfa6c6f127cdb0802ac8b97c0117482f2305@1"
                .to_string(),
            recipient: OmniAddress::Near(account_n(1)),
            relayer_fee: U128(1000),
            msg: String::default(),
        }
    }

    async fn do_utxo_fin_transfer(
        env: &TestEnv,
        amount: u128,
        utxo_msg: UtxoFinTransferMsg,
        error: Option<&str>,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let result = env
            .relayer_account
            .call(env.utxo_connector.id(), "verify_deposit")
            .args_json(json!({
                "amount": U128(amount),
                "msg": utxo_msg,
            }))
            .max_gas()
            .transact()
            .await?;

        if let Some(expected_error) = error {
            assert!(has_error_message(&result, expected_error));
        } else {
            assert_eq!(0, result.failures().len());
        }

        Ok(result)
    }

    async fn do_fast_transfer(
        env: &TestEnv,
        amount: u128,
        utxo_msg: UtxoFinTransferMsg,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let fast_transfer_msg = FastFinTransferMsg {
            transfer_id: UnifiedTransferId {
                origin_chain: ChainKind::Btc,
                id: ChainTransferId::Utxo(utxo_msg.utxo_id.clone()),
            },
            recipient: utxo_msg.recipient.clone(),
            fee: Fee {
                fee: utxo_msg.relayer_fee,
                native_fee: U128(0),
            },
            amount: U128(amount),
            msg: utxo_msg.msg.clone(),
            storage_deposit_amount: None,
            relayer: env.relayer_account.id().clone(),
        };

        let required_storage: NearToken = env
            .bridge_contract
            .view("required_balance_for_fast_transfer")
            .await?
            .json()?;

        env.relayer_account
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({
                "account_id": env.relayer_account.id(),
            }))
            .deposit(required_storage)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Perform fast transfer
        let result = env.relayer_account
                .call(env.token_contract.id(), "ft_transfer_call")
                .args_json(json!({
                    "receiver_id": env.bridge_contract.id(),
                    "amount": U128(amount - utxo_msg.relayer_fee.0),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&BridgeOnTransferMsg::FastFinTransfer(fast_transfer_msg))?,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?;

        assert!(result.failures().is_empty(), "Fast transfer should succeed");

        Ok(result)
    }

    mod transfer_to_near {
        use super::*;

        #[rstest]
        #[tokio::test]
        async fn succeeds_basic(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts).await?;
            let amount = 100_000_000;
            let utxo_msg = default_utxo_fin_transfer();

            let OmniAddress::Near(recipient) = utxo_msg.recipient.clone() else {
                panic!("Expected Near recipient");
            };

            let recipient_balance_before = get_balance(&env.token_contract, &recipient).await?;

            let _ = do_utxo_fin_transfer(&env, amount, utxo_msg, None).await?;

            let recipient_balance_after = get_balance(&env.token_contract, &recipient).await?;
            assert_eq!(
                amount,
                recipient_balance_after.0 - recipient_balance_before.0
            );

            Ok(())
        }

        #[rstest]
        #[tokio::test]
        async fn fails_when_sender_is_not_connector(
            build_artifacts: &BuildArtifacts,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts).await?;
            let amount = 100_000_000;
            let utxo_msg = default_utxo_fin_transfer();

            // Try to send from relayer (not the connector)
            let result = env
                .relayer_account
                .call(env.token_contract.id(), "ft_transfer_call")
                .args_json(json!({
                    "receiver_id": env.bridge_contract.id(),
                    "amount": U128(amount),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&BridgeOnTransferMsg::UtxoFinTransfer(utxo_msg))?,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?;

            assert!(has_error_message(&result, "ERR_SENDER_IS_NOT_CONNECTOR"));

            Ok(())
        }

        #[rstest]
        #[tokio::test]
        async fn refunds_when_recipient_not_registered(
            build_artifacts: &BuildArtifacts,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts).await?;
            let amount = 100_000_000;
            let utxo_msg = default_utxo_fin_transfer();
            let recipient = env.recipient_account.id();
            assert!(OmniAddress::Near(recipient.clone()) == utxo_msg.recipient);

            // Unregister the recipient
            env.recipient_account
                .call(env.token_contract.id(), "storage_unregister")
                .args_json(json!({
                    "account_id": &recipient,
                    "force": true,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let connector_balance_before =
                get_balance(&env.token_contract, env.utxo_connector.id()).await?;
            let bridge_balance_before =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            let _ =
                do_utxo_fin_transfer(&env, amount, utxo_msg, Some("recipient is omitted")).await?;

            let connector_balance_after =
                get_balance(&env.token_contract, env.utxo_connector.id()).await?;
            let bridge_balance_after =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            assert_eq!(
                connector_balance_before.0, connector_balance_after.0,
                "Connector balance should be unchanged after refund"
            );

            assert_eq!(
                bridge_balance_before.0, bridge_balance_after.0,
                "Bridge should not have received tokens"
            );

            let recipient_balance = get_balance(&env.token_contract, &recipient).await?;
            assert_eq!(0, recipient_balance.0, "Recipient should have zero balance");

            Ok(())
        }
    }

    mod transfer_to_other_chain {
        use super::*;

        #[rstest]
        #[tokio::test]
        async fn succeeds_creates_transfer_message(
            build_artifacts: &BuildArtifacts,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts).await?;
            let amount = 100_000_000;
            let mut utxo_msg = default_utxo_fin_transfer();
            utxo_msg.recipient = base_eoa_address();

            let _ = do_utxo_fin_transfer(&env, amount, utxo_msg, None).await?;

            // Verify a transfer message was created
            let transfer_message: Option<omni_types::TransferMessage> = env
                .bridge_contract
                .view("get_transfer_message")
                .args_json(json!({
                    "transfer_id": omni_types::TransferId {
                        origin_chain: ChainKind::Near,
                        origin_nonce: 1,
                    },
                }))
                .await
                .ok()
                .and_then(|r| r.json().ok());

            assert!(transfer_message.is_some());
            let transfer_message = transfer_message.unwrap();
            assert_eq!(transfer_message.amount.0, amount);
            assert_eq!(transfer_message.recipient, base_eoa_address());

            Ok(())
        }
    }

    mod fast_transfer_finalization {
        use super::*;

        #[rstest]
        #[tokio::test]
        async fn succeeds_for_near(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts).await?;
            let amount = 100_000_000;
            let utxo_msg = default_utxo_fin_transfer();

            let _ = do_fast_transfer(&env, amount, utxo_msg.clone()).await?;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;

            let _ = do_utxo_fin_transfer(&env, amount, utxo_msg, None).await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;

            assert_eq!(
                amount,
                relayer_balance_after.0 - relayer_balance_before.0,
                "Relayer should receive full amount"
            );

            Ok(())
        }

        #[rstest]
        #[tokio::test]
        async fn succeeds_for_other_chain(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts).await?;
            let amount = 100_000_000;
            let mut utxo_msg = default_utxo_fin_transfer();
            let fee = utxo_msg.relayer_fee.0;
            utxo_msg.recipient = base_eoa_address();

            let _ = do_fast_transfer(&env, amount, utxo_msg.clone()).await?;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;

            let _ = do_utxo_fin_transfer(&env, amount, utxo_msg, None).await?;

            // Verify relayer received amount minus fee
            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            assert_eq!(
                amount - fee,
                relayer_balance_after.0 - relayer_balance_before.0,
                "Relayer should receive amount minus fee for other chain transfers"
            );

            Ok(())
        }
    }
}
