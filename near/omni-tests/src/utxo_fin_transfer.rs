#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{result::ExecutionFinalResult, types::NearToken};
    use omni_types::{
        BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, Fee, OmniAddress, TransferIdKind,
        UnifiedTransferId, UtxoFinTransferMsg,
    };
    use rstest::rstest;

    use crate::helpers::tests::{account_n, base_eoa_address, build_artifacts};
    use crate::{environment::*, helpers::tests::BuildArtifacts};

    struct UtxoFinTransferCase {
        amount: u128,
        utxo_msg: UtxoFinTransferMsg,
        is_fast_transfer: bool,
        error: Option<&'static str>,
    }

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
            env_builder
                .bridge_contract
                .call("set_locked_token")
                .args_json(json!({
                    "args": {
                        "chain_kind": ChainKind::Near,
                        "token_id": env_builder.token.contract.id(),
                        "amount": U128(1_000_000_000),
                    }
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

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

    async fn get_locked_tokens(
        bridge_contract: &near_workspaces::Contract,
        chain_kind: ChainKind,
        token_id: &AccountId,
    ) -> anyhow::Result<U128> {
        let locked_tokens: U128 = bridge_contract
            .view("get_locked_tokens")
            .args_json(json!({
                "chain_kind": chain_kind,
                "token_id": token_id,
            }))
            .await?
            .json()?;

        Ok(locked_tokens)
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

    fn default_utxo_id() -> omni_types::UtxoId {
        omni_types::UtxoId {
            tx_hash: "abc94fc5b954136a691594c7044bcfa6c6f127cdb0802ac8b97c0117482f2305".to_string(),
            vout: 1,
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn do_utxo_fin_transfer(
        env: &TestEnv,
        amount: u128,
        utxo_msg: UtxoFinTransferMsg,
        is_fast_transfer: bool,
        error: Option<&str>,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let is_transfer_to_near = matches!(utxo_msg.recipient, OmniAddress::Near(_));

        let locked_before = get_locked_tokens(
            &env.bridge_contract,
            ChainKind::Near,
            env.token_contract.id(),
        )
        .await?;
        let connector_balance_before =
            get_balance(&env.token_contract, env.utxo_connector.id()).await?;
        let recipient_balance_before =
            get_balance(&env.token_contract, env.recipient_account.id()).await?;
        let relayer_balance_before =
            get_balance(&env.token_contract, env.relayer_account.id()).await?;

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

        let connector_balance_after =
            get_balance(&env.token_contract, env.utxo_connector.id()).await?;
        let recipient_balance_after =
            get_balance(&env.token_contract, env.recipient_account.id()).await?;
        let relayer_balance_after =
            get_balance(&env.token_contract, env.relayer_account.id()).await?;
        let locked_after = get_locked_tokens(
            &env.bridge_contract,
            ChainKind::Near,
            env.token_contract.id(),
        )
        .await?;

        if let Some(expected_error) = error {
            assert!(has_error_message(&result, expected_error));

            assert_eq!(
                connector_balance_before.0, connector_balance_after.0,
                "Connector balance should be unchanged after failed transfer"
            );
            assert_eq!(
                relayer_balance_before.0, relayer_balance_after.0,
                "Relayer balance should be unchanged after failed transfer"
            );
            assert_eq!(
                recipient_balance_before.0, recipient_balance_after.0,
                "Recipient balance should be unchanged after failed transfer"
            );
        } else {
            assert!(
                result.failures().is_empty(),
                "Unexpected failures: {:?}",
                result.failures()
            );

            assert_eq!(
                connector_balance_before.0,
                connector_balance_after.0 + amount,
                "Connector balance is not correct"
            );

            let (recipient_change, relayer_change) = match (is_fast_transfer, is_transfer_to_near) {
                (true, true) => (0, amount),
                (true, false) => (0, amount - utxo_msg.relayer_fee.0),
                (false, true) => (amount, 0),
                (false, false) => (0, 0),
            };

            assert_eq!(
                relayer_balance_before.0,
                relayer_balance_after.0 - relayer_change,
                "Relayer balance is not correct"
            );
            assert_eq!(
                recipient_balance_before.0,
                recipient_balance_after.0 - recipient_change,
                "Recipient balance is not correct"
            );
        }

        assert_eq!(
            locked_before, locked_after,
            "Locked tokens should be unchanged on Near"
        );

        if !is_fast_transfer && !is_transfer_to_near {
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

            if error.is_none() {
                assert!(transfer_message.is_some());
                let transfer_message = transfer_message.unwrap();
                assert_eq!(transfer_message.amount.0, amount);
                assert_eq!(transfer_message.recipient, base_eoa_address());
            } else {
                assert!(transfer_message.is_none());
            }
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
                kind: TransferIdKind::Utxo(utxo_msg.utxo_id.clone()),
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

    #[rstest]
    // Succeeds when transferring to Near
    #[case(
        UtxoFinTransferCase {
            amount: 100_000_000,
            utxo_msg: UtxoFinTransferMsg {
                utxo_id: default_utxo_id(),
                recipient: OmniAddress::Near(account_n(1)),
                relayer_fee: U128(1000),
                msg: String::default(),
            },
            is_fast_transfer: false,
            error: None,
        }
    )]
    // Succeeds when transferring to other chain
    #[case(
        UtxoFinTransferCase {
            amount: 100_000_000,
            utxo_msg: UtxoFinTransferMsg {
                utxo_id: default_utxo_id(),
                recipient: base_eoa_address(),
                relayer_fee: U128(1000),
                msg: String::default(),
            },
            is_fast_transfer: false,
            error: None,
        }
    )]
    // Refunds if token transfer fails
    #[case(
        UtxoFinTransferCase {
            amount: 100_000_000,
            utxo_msg: UtxoFinTransferMsg {
                utxo_id: default_utxo_id(),
                recipient: OmniAddress::Near(account_n(1)),
                relayer_fee: U128(2000),
                msg: "Some_message".to_string(),
            },
            is_fast_transfer: false,
            error: Some("CodeDoesNotExist"),
        }
    )]
    // Succeeds after fast transfer to Near
    #[case(
        UtxoFinTransferCase {
            amount: 100_000_000,
            utxo_msg: UtxoFinTransferMsg {
                utxo_id: default_utxo_id(),
                recipient: OmniAddress::Near(account_n(1)),
                relayer_fee: U128(1000),
                msg: String::default(),
            },
            is_fast_transfer: true,
            error: None,
        }
    )]
    // Succeeds after fast transfer to other chain
    #[case(
        UtxoFinTransferCase {
            amount: 100_000_000,
            utxo_msg: UtxoFinTransferMsg {
                utxo_id: default_utxo_id(),
                recipient: base_eoa_address(),
                relayer_fee: U128(1000),
                msg: String::default(),
            },
            is_fast_transfer: true,
            error: None,
        }
    )]
    // Refunds when recipient not registered
    #[case(
        UtxoFinTransferCase {
            amount: 100_000_000,
            utxo_msg: UtxoFinTransferMsg {
                utxo_id: default_utxo_id(),
                recipient: OmniAddress::Near(account_n(3)),
                relayer_fee: U128(1000),
                msg: String::default(),
            },
            is_fast_transfer: false,
            error: Some("ERR_STORAGE_RECIPIENT_OMITTED"),
        }
    )]
    #[tokio::test]
    async fn normal_call(
        build_artifacts: &BuildArtifacts,
        #[case] case: UtxoFinTransferCase,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(build_artifacts).await?;

        if case.is_fast_transfer {
            let _ = do_fast_transfer(&env, case.amount, case.utxo_msg.clone()).await?;
        }

        let _ = do_utxo_fin_transfer(
            &env,
            case.amount,
            case.utxo_msg,
            case.is_fast_transfer,
            case.error,
        )
        .await?;

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn fails_when_sender_is_not_connector(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(build_artifacts).await?;
        let amount = 100_000_000;
        let utxo_msg = UtxoFinTransferMsg {
            utxo_id: default_utxo_id(),
            recipient: OmniAddress::Near(account_n(1)),
            relayer_fee: U128(1000),
            msg: String::default(),
        };

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
    async fn fails_on_double_finalization(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let env = TestEnv::new(build_artifacts).await?;
        let amount = 100_000_000;
        let utxo_msg = UtxoFinTransferMsg {
            utxo_id: default_utxo_id(),
            recipient: base_eoa_address(),
            relayer_fee: U128(1000),
            msg: String::default(),
        };

        let _ = do_fast_transfer(&env, amount, utxo_msg.clone()).await?;

        let _ = do_utxo_fin_transfer(&env, amount, utxo_msg.clone(), true, None).await?;
        let _ = do_utxo_fin_transfer(
            &env,
            amount,
            utxo_msg,
            true,
            Some("ERR_FAST_TRANSFER_ALREADY_FINALISED"),
        )
        .await?;

        Ok(())
    }
}
