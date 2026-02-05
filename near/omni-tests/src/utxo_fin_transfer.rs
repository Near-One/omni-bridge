#[cfg(test)]
mod tests {
    use near_api::{AccountId, NetworkConfig};
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
    };
    use near_token::NearToken;
    use omni_types::{
        BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, Fee, OmniAddress, TransferIdKind,
        UnifiedTransferId, UtxoFinTransferMsg,
    };
    use rstest::rstest;

    use crate::helpers::tests::{account_n, base_eoa_address, build_artifacts};
    use crate::{
        environment::{TestAccount, TestContract, TestEnvBuilder, TransactionResult},
        helpers::tests::BuildArtifacts,
    };

    struct UtxoFinTransferCase {
        amount: u128,
        utxo_msg: UtxoFinTransferMsg,
        is_fast_transfer: bool,
        error: Option<&'static str>,
    }

    struct TestEnv {
        #[allow(dead_code)]
        sandbox: near_sandbox::Sandbox, // Keep sandbox alive for the duration of the test
        network: NetworkConfig,
        token_contract: TestContract,
        bridge_contract: TestContract,
        utxo_connector: TestContract,
        relayer_account: TestAccount,
        recipient_account: TestAccount,
    }

    impl TestEnv {
        async fn new(build_artifacts: &BuildArtifacts) -> anyhow::Result<Self> {
            let env_builder = TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .with_utxo_token()
                .await?;

            let relayer_account = env_builder.create_account(account_n(10)).await?;
            env_builder.storage_deposit(&relayer_account.id).await?;
            env_builder
                .omni_storage_deposit(&relayer_account.id, 1_000_000_000_000_000_000_000_000)
                .await?;
            env_builder
                .mint_tokens(&relayer_account.id, 1_000_000_000)
                .await?;

            let recipient_account = env_builder.create_account(account_n(1)).await?;
            env_builder.storage_deposit(&recipient_account.id).await?;

            env_builder
                .mint_tokens(
                    &env_builder.utxo_connector.as_ref().unwrap().id,
                    1_000_000_000_000,
                )
                .await?;

            Ok(Self {
                sandbox: env_builder.sandbox,
                network: env_builder.network,
                token_contract: env_builder.token.contract,
                bridge_contract: env_builder.bridge_contract,
                utxo_connector: env_builder.utxo_connector.unwrap(),
                relayer_account,
                recipient_account,
            })
        }
    }

    async fn get_balance(
        token_contract: &TestContract,
        account_id: &AccountId,
        network: &NetworkConfig,
    ) -> anyhow::Result<U128> {
        let balance: U128 = token_contract
            .view("ft_balance_of", json!({ "account_id": account_id }), network)
            .await?;

        Ok(balance)
    }

    fn has_error_message(result: &TransactionResult, error_msg: &str) -> bool {
        let has_failure = result.failures().iter().any(|outcome| {
            (*outcome)
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

    async fn do_utxo_fin_transfer(
        env: &TestEnv,
        amount: u128,
        utxo_msg: UtxoFinTransferMsg,
        is_fast_transfer: bool,
        error: Option<&str>,
    ) -> anyhow::Result<TransactionResult> {
        let is_transfer_to_near = matches!(utxo_msg.recipient, OmniAddress::Near(_));

        let connector_balance_before =
            get_balance(&env.token_contract, &env.utxo_connector.id, &env.network).await?;
        let recipient_balance_before =
            get_balance(&env.token_contract, &env.recipient_account.id, &env.network).await?;
        let relayer_balance_before =
            get_balance(&env.token_contract, &env.relayer_account.id, &env.network).await?;

        let result = env
            .utxo_connector
            .call_by(
                &env.relayer_account.id,
                &env.relayer_account.signer,
                "verify_deposit",
                json!({
                    "amount": U128(amount),
                    "msg": utxo_msg,
                }),
                NearToken::from_yoctonear(0),
                &env.network,
            )
            .await?;

        let connector_balance_after =
            get_balance(&env.token_contract, &env.utxo_connector.id, &env.network).await?;
        let recipient_balance_after =
            get_balance(&env.token_contract, &env.recipient_account.id, &env.network).await?;
        let relayer_balance_after =
            get_balance(&env.token_contract, &env.relayer_account.id, &env.network).await?;

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
            assert_eq!(0, result.failures().len());

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

        if !is_fast_transfer && !is_transfer_to_near {
            let transfer_message: Option<omni_types::TransferMessage> = env
                .bridge_contract
                .view(
                    "get_transfer_message",
                    json!({
                        "transfer_id": omni_types::TransferId {
                            origin_chain: ChainKind::Near,
                            origin_nonce: 1,
                        },
                    }),
                    &env.network,
                )
                .await
                .ok();

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
    ) -> anyhow::Result<TransactionResult> {
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
            relayer: env.relayer_account.id.clone(),
        };

        let required_storage: NearToken = env
            .bridge_contract
            .view_no_args("required_balance_for_fast_transfer", &env.network)
            .await?;

        env.bridge_contract
            .call_by(
                &env.relayer_account.id,
                &env.relayer_account.signer,
                "storage_deposit",
                json!({
                    "account_id": env.relayer_account.id,
                }),
                required_storage,
                &env.network,
            )
            .await?;

        // Perform fast transfer
        let result = env
            .token_contract
            .call_by(
                &env.relayer_account.id,
                &env.relayer_account.signer,
                "ft_transfer_call",
                json!({
                    "receiver_id": env.bridge_contract.id,
                    "amount": U128(amount - utxo_msg.relayer_fee.0),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&BridgeOnTransferMsg::FastFinTransfer(fast_transfer_msg))?,
                }),
                NearToken::from_yoctonear(1),
                &env.network,
            )
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
            error: Some("recipient is omitted"),
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
            .token_contract
            .call_by(
                &env.relayer_account.id,
                &env.relayer_account.signer,
                "ft_transfer_call",
                json!({
                    "receiver_id": env.bridge_contract.id,
                    "amount": U128(amount),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&BridgeOnTransferMsg::UtxoFinTransfer(utxo_msg))?,
                }),
                NearToken::from_yoctonear(1),
                &env.network,
            )
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
