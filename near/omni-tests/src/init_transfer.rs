#[cfg(test)]
mod tests {
    use anyhow::Ok;
    use near_api::{Contract as ApiContract, NetworkConfig};
    use near_sandbox::Sandbox;
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
    };
    use near_token::NearToken;
    use omni_types::{
        near_events::OmniBridgeEvent, BridgeOnTransferMsg, ChainKind, Fee, InitTransferMsg,
        OmniAddress, TransferId, TransferMessage, UpdateFee,
    };
    use rstest::rstest;

    use crate::{
        environment::{TestAccount, TestContract, TransactionResult},
        helpers::tests::{
            account_n, build_artifacts, eth_eoa_address, eth_factory_address,
            get_claim_fee_args_near, get_event_data, relayer_account_id, BuildArtifacts,
        },
    };

    const DEFAULT_NEAR_SANDBOX_BALANCE: NearToken = NearToken::from_near(100);
    const EXPECTED_RELAYER_GAS_COST: NearToken =
        NearToken::from_yoctonear(1_500_000_000_000_000_000_000);

    #[allow(dead_code)]
    struct TestEnv {
        sandbox: Sandbox,
        network: NetworkConfig,
        token_contract: TestContract,
        locker_contract: TestContract,
        relayer_account: TestAccount,
        sender_account: TestAccount,
        eth_factory_address: OmniAddress,
        build_artifacts: BuildArtifacts,
    }

    impl TestEnv {
        #[allow(clippy::too_many_lines)]
        async fn new(
            sender_balance_token: u128,
            is_old_locker: bool,
            build_artifacts: &BuildArtifacts,
        ) -> anyhow::Result<Self> {
            let env_builder = crate::environment::TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .deploy_old_version(is_old_locker)
                .with_native_nep141_token(24)
                .await?;
            let relayer_account = env_builder.create_account(relayer_account_id()).await?;
            let sender_account = env_builder.create_account(account_n(1)).await?;
            env_builder.storage_deposit(&relayer_account.id).await?;
            env_builder.storage_deposit(&sender_account.id).await?;
            env_builder
                .mint_tokens(&sender_account.id, sender_balance_token)
                .await?;

            Ok(Self {
                sandbox: env_builder.sandbox,
                network: env_builder.network,
                token_contract: env_builder.token.contract,
                locker_contract: env_builder.bridge_contract,
                relayer_account,
                sender_account,
                eth_factory_address: eth_factory_address(),
                build_artifacts: build_artifacts.clone(),
            })
        }
    }

    async fn init_transfer_legacy(
        env: &TestEnv,
        transfer_amount: u128,
        init_transfer_msg: InitTransferMsg,
    ) -> anyhow::Result<TransferMessage> {
        let storage_deposit_amount = get_balance_required_for_account(
            &env.locker_contract,
            &env.sender_account,
            &init_transfer_msg,
            None,
            &env.network,
        )
        .await?;

        // Storage deposit
        env.locker_contract
            .call_by(
                &env.sender_account.id,
                &env.sender_account.signer,
                "storage_deposit",
                json!({
                    "account_id": env.sender_account.id,
                }),
                storage_deposit_amount,
                &env.network,
            )
            .await?;

        // Initiate the transfer
        let transfer_result = env
            .token_contract
            .call_by(
                &env.sender_account.id,
                &env.sender_account.signer,
                "ft_transfer_call",
                json!({
                    "receiver_id": env.locker_contract.id,
                    "amount": U128(transfer_amount),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&init_transfer_msg)?,
                }),
                NearToken::from_yoctonear(1),
                &env.network,
            )
            .await?;

        // Ensure the transfer event is emitted
        let transfer_message = get_transfer_message_from_event(&transfer_result)?;

        Ok(transfer_message)
    }

    async fn init_transfer_flow_on_near(
        env: &TestEnv,
        transfer_amount: u128,
        init_transfer_msg: InitTransferMsg,
        custom_deposit: Option<NearToken>,
        update_fee: Option<UpdateFee>,
        is_relayer_sign: bool,
    ) -> anyhow::Result<()> {
        let storage_deposit_amount = get_balance_required_for_account(
            &env.locker_contract,
            &env.sender_account,
            &init_transfer_msg,
            custom_deposit,
            &env.network,
        )
        .await?;

        // Deposit to the storage
        env.locker_contract
            .call_by(
                &env.sender_account.id,
                &env.sender_account.signer,
                "storage_deposit",
                json!({
                    "account_id": env.sender_account.id,
                }),
                storage_deposit_amount,
                &env.network,
            )
            .await?;

        // Initiate the transfer
        let transfer_result = env
            .token_contract
            .call_by(
                &env.sender_account.id,
                &env.sender_account.signer,
                "ft_transfer_call",
                json!({
                    "receiver_id": env.locker_contract.id,
                    "amount": U128(transfer_amount),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(init_transfer_msg))?,
                }),
                NearToken::from_yoctonear(1),
                &env.network,
            )
            .await?;

        // Ensure the transfer event is emitted
        let transfer_message = get_transfer_message_from_event(&transfer_result)?;

        // Update the transfer fee if needed
        let signing_fee = if let Some(update_fee) = update_fee.clone() {
            make_fee_update(
                update_fee.clone(),
                &transfer_message,
                &env.locker_contract,
                &env.sender_account,
                &env.network,
            )
            .await?;
            match update_fee {
                UpdateFee::Fee(new_fee) => new_fee,
                UpdateFee::Proof(_) => transfer_message.fee.clone(),
            }
        } else {
            transfer_message.fee.clone()
        };

        // Transfer is signed by the relayer or the sender
        let signer = if is_relayer_sign {
            &env.relayer_account
        } else {
            &env.sender_account
        };

        env.locker_contract
            .call_by(
                &signer.id,
                &signer.signer,
                "sign_transfer",
                json!({
                    "transfer_id": TransferId {
                        origin_chain: ChainKind::Near,
                        origin_nonce: transfer_message.origin_nonce,
                    },
                    "fee_recipient": env.relayer_account.id,
                    "fee": &Some(signing_fee.clone()),
                }),
                NearToken::from_yoctonear(0),
                &env.network,
            )
            .await?;

        // Relayer claims the fee
        let claim_fee_args = get_claim_fee_args_near(
            ChainKind::Near,
            ChainKind::Eth,
            transfer_message.origin_nonce,
            &env.relayer_account.id,
            transfer_amount - signing_fee.fee.0,
            env.eth_factory_address.clone(),
        );
        env.locker_contract
            .call_borsh_by(
                &env.relayer_account.id,
                &env.relayer_account.signer,
                "claim_fee",
                near_sdk::borsh::to_vec(&claim_fee_args)?,
                NearToken::from_yoctonear(1),
                &env.network,
            )
            .await?;
        Ok(())
    }

    async fn get_balance_required_for_account(
        locker_contract: &TestContract,
        sender_account: &TestAccount,
        init_transfer_msg: &InitTransferMsg,
        custom_deposit: Option<NearToken>,
        network: &NetworkConfig,
    ) -> anyhow::Result<NearToken> {
        let required_balance_account: NearToken = locker_contract
            .view_no_args("required_balance_for_account", network)
            .await?;

        let required_balance_init_transfer: NearToken = locker_contract
            .view(
                "required_balance_for_init_transfer",
                json!({
                    "recipient": init_transfer_msg.recipient,
                    "sender": OmniAddress::Near(sender_account.id.clone()),
                }),
                network,
            )
            .await?;

        let storage_deposit_amount = match custom_deposit {
            Some(deposit) => deposit,
            None => required_balance_account
                .saturating_add(NearToken::from_yoctonear(
                    init_transfer_msg.native_token_fee.0,
                ))
                .saturating_add(required_balance_init_transfer),
        };

        Ok(storage_deposit_amount)
    }

    fn get_transfer_message_from_event(
        transfer_result: &TransactionResult,
    ) -> anyhow::Result<TransferMessage> {
        let logs = transfer_result
            .logs()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let logs_refs = logs.iter().collect::<Vec<_>>();

        let omni_bridge_event: OmniBridgeEvent = serde_json::from_value(
            get_event_data("InitTransferEvent", &logs_refs)?
                .ok_or_else(|| anyhow::anyhow!("InitTransferEvent not found"))?,
        )?;
        let OmniBridgeEvent::InitTransferEvent { transfer_message } = omni_bridge_event else {
            anyhow::bail!("InitTransferEvent is found in unexpected event")
        };

        Ok(transfer_message)
    }

    async fn make_fee_update(
        update_fee: UpdateFee,
        transfer_message: &TransferMessage,
        locker_contract: &TestContract,
        sender_account: &TestAccount,
        network: &NetworkConfig,
    ) -> anyhow::Result<()> {
        let deposit = match update_fee.clone() {
            UpdateFee::Fee(update_fee) => NearToken::from_yoctonear(
                update_fee
                    .native_fee
                    .0
                    .saturating_sub(transfer_message.fee.native_fee.0),
            ),
            // To be updated once the proof is implemented
            UpdateFee::Proof(_) => NearToken::from_yoctonear(0),
        };
        locker_contract
            .call_by(
                &sender_account.id,
                &sender_account.signer,
                "update_transfer_fee",
                json!({
                    "transfer_id": TransferId {
                        origin_chain: ChainKind::Near,
                        origin_nonce: transfer_message.origin_nonce,
                    },
                    "fee": update_fee.clone(),
                }),
                deposit,
                network,
            )
            .await?;
        Ok(())
    }

    async fn get_token_balance(
        token_contract: &TestContract,
        account_id: &near_api::AccountId,
        network: &NetworkConfig,
    ) -> anyhow::Result<U128> {
        Ok(token_contract
            .view("ft_balance_of", json!({ "account_id": account_id }), network)
            .await?)
    }

    async fn get_test_balances(env: &TestEnv) -> anyhow::Result<(U128, U128, U128, NearToken)> {
        let user_balance_token: U128 =
            get_token_balance(&env.token_contract, &env.sender_account.id, &env.network).await?;
        let locker_balance_token: U128 =
            get_token_balance(&env.token_contract, &env.locker_contract.id, &env.network).await?;
        let relayer_balance_token: U128 =
            get_token_balance(&env.token_contract, &env.relayer_account.id, &env.network).await?;
        let relayer_balance_near: NearToken = near_api::Account(env.relayer_account.id.clone())
            .view()
            .fetch_from(&env.network)
            .await?
            .data
            .amount;

        Ok((
            user_balance_token,
            locker_balance_token,
            relayer_balance_token,
            relayer_balance_near,
        ))
    }

    #[rstest]
    #[tokio::test]
    async fn test_native_fee(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 100;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(0),
            recipient: eth_eoa_address(),
            msg: None,
        };

        let env = TestEnv::new(sender_balance_token, false, build_artifacts).await?;

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            None,
            false,
        )
        .await?;

        let (user_balance_token, locker_balance_token, _, relayer_balance_near) =
            get_test_balances(&env).await?;

        assert_eq!(
            user_balance_token,
            U128(sender_balance_token - transfer_amount),
            "User balance was not deducted"
        );
        assert_eq!(
            locker_balance_token,
            U128(transfer_amount),
            "Locker balance was not increased"
        );
        assert!(
            DEFAULT_NEAR_SANDBOX_BALANCE.as_yoctonear()
                + NearToken::from_yoctonear(init_transfer_msg.native_token_fee.0).as_yoctonear()
                - relayer_balance_near.as_yoctonear()
                < EXPECTED_RELAYER_GAS_COST.as_yoctonear(),
            "Relayer didn't receive native fee."
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_transfer_fee(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(0),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
        };

        let env = TestEnv::new(sender_balance_token, false, build_artifacts).await?;

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            None,
            false,
        )
        .await?;

        let (user_balance_token, locker_balance_token, relayer_balance_token, _) =
            get_test_balances(&env).await?;

        assert_eq!(
            user_balance_token,
            U128(sender_balance_token - transfer_amount),
            "User balance was not deducted"
        );
        assert_eq!(
            locker_balance_token,
            U128(transfer_amount - init_transfer_msg.fee.0),
            "Locker balance was not increased or the fee was not deducted"
        );
        assert_eq!(
            relayer_balance_token,
            U128(init_transfer_msg.fee.0),
            "Relayer didn't receive transfer fee."
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_both_fee(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
        };

        let env = TestEnv::new(sender_balance_token, false, build_artifacts).await?;

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            None,
            false,
        )
        .await?;

        let (user_balance_token, locker_balance_token, relayer_balance_token, relayer_balance_near) =
            get_test_balances(&env).await?;

        assert_eq!(
            user_balance_token,
            U128(sender_balance_token - transfer_amount),
            "User balance was not deducted"
        );
        assert_eq!(
            locker_balance_token,
            U128(transfer_amount - init_transfer_msg.fee.0),
            "Locker balance was not increased or the fee was not deducted"
        );

        assert!(
            DEFAULT_NEAR_SANDBOX_BALANCE.as_yoctonear()
                + NearToken::from_yoctonear(init_transfer_msg.native_token_fee.0).as_yoctonear()
                - relayer_balance_near.as_yoctonear()
                < EXPECTED_RELAYER_GAS_COST.as_yoctonear(),
            "Relayer didn't receive native fee."
        );

        assert_eq!(
            relayer_balance_token,
            U128(init_transfer_msg.fee.0),
            "Relayer didn't receive transfer fee."
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_update_fee(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(2).as_yoctonear()),
            fee: U128(2000),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token, false, build_artifacts).await?;

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            Some(update_fee),
            false,
        )
        .await?;

        let (user_balance_token, locker_balance_token, relayer_balance_token, relayer_balance_near) =
            get_test_balances(&env).await?;

        assert_eq!(
            user_balance_token,
            U128(sender_balance_token - transfer_amount),
            "User balance was not deducted"
        );
        assert_eq!(
            locker_balance_token,
            U128(transfer_amount - update_fee_value.fee.0),
            "Locker balance was not increased or the fee was not deducted"
        );

        assert!(
            DEFAULT_NEAR_SANDBOX_BALANCE.as_yoctonear()
                + NearToken::from_yoctonear(update_fee_value.native_fee.0).as_yoctonear()
                - relayer_balance_near.as_yoctonear()
                < EXPECTED_RELAYER_GAS_COST.as_yoctonear(),
            "Relayer didn't receive native fee."
        );

        assert_eq!(
            relayer_balance_token,
            U128(update_fee_value.fee.0),
            "Relayer didn't receive transfer fee."
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_relayer_sign(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 100;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(0),
            recipient: eth_eoa_address(),
            msg: None,
        };

        let env = TestEnv::new(sender_balance_token, false, build_artifacts).await?;

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            None,
            true,
        )
        .await?;

        let (user_balance_token, locker_balance_token, _, relayer_balance_near) =
            get_test_balances(&env).await?;

        assert_eq!(
            user_balance_token,
            U128(sender_balance_token - transfer_amount),
            "User balance was not deducted"
        );
        assert_eq!(
            locker_balance_token,
            U128(transfer_amount),
            "Locker balance was not increased"
        );

        assert!(
            DEFAULT_NEAR_SANDBOX_BALANCE.as_yoctonear()
                + NearToken::from_yoctonear(init_transfer_msg.native_token_fee.0).as_yoctonear()
                - relayer_balance_near.as_yoctonear()
                < EXPECTED_RELAYER_GAS_COST.as_yoctonear(),
            "Relayer didn't receive native fee."
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    #[should_panic(expected = "ERR_LOWER_FEE")]
    async fn test_update_fee_native_too_small(build_artifacts: &BuildArtifacts) {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(0).as_yoctonear()),
            fee: U128(2000),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token, false, build_artifacts)
            .await
            .unwrap();

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            Some(update_fee),
            false,
        )
        .await
        .unwrap();
    }

    #[rstest]
    #[tokio::test]
    #[should_panic(expected = "ERR_INVALID_FEE")]
    async fn test_update_fee_transfer_fee_too_small(build_artifacts: &BuildArtifacts) {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(500),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token, false, build_artifacts)
            .await
            .unwrap();

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            Some(update_fee),
            false,
        )
        .await
        .unwrap();
    }

    #[rstest]
    #[tokio::test]
    #[should_panic(expected = "ERR_INVALID_FEE")]
    async fn test_update_fee_transfer_fee_too_big(build_artifacts: &BuildArtifacts) {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(6000),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token, false, build_artifacts)
            .await
            .unwrap();

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            Some(update_fee),
            false,
        )
        .await
        .unwrap();
    }

    #[rstest]
    #[tokio::test]
    #[should_panic(expected = "TODO")]
    // Add a test once the Proof update fee is implemented
    async fn test_update_fee_proof(build_artifacts: &BuildArtifacts) {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
        };
        let update_fee = UpdateFee::Proof(vec![]);

        let env = TestEnv::new(sender_balance_token, false, build_artifacts)
            .await
            .unwrap();

        init_transfer_flow_on_near(
            &env,
            transfer_amount,
            init_transfer_msg.clone(),
            None,
            Some(update_fee),
            false,
        )
        .await
        .unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn test_migrate(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(0),
            fee: U128(0),
            recipient: eth_eoa_address(),
            msg: None,
        };

        let env = TestEnv::new(sender_balance_token, true, build_artifacts).await?;

        let transfer_message =
            init_transfer_legacy(&env, transfer_amount, init_transfer_msg.clone()).await?;

        // Deploy new locker code
        let locker_account = TestAccount {
            id: env.locker_contract.id.clone(),
            signer: env.locker_contract.signer.clone(),
        };
        let res = locker_account
            .deploy(&env.build_artifacts.locker, &env.network)
            .await?;

        assert!(res.is_success(), "Failed to upgrade locker");

        // Call migrate
        let res = env
            .locker_contract
            .call("migrate", json!({}), NearToken::from_yoctonear(0), &env.network)
            .await?;

        assert!(res.is_success(), "Migration didn't succeed");

        // Verify the migrated transfer
        let migrated_transfer: TransferMessage = ApiContract(env.locker_contract.id.clone())
            .call_function(
                "get_transfer_message",
                json!({
                    "transfer_id": TransferId {
                        origin_chain: ChainKind::Near,
                        origin_nonce: transfer_message.origin_nonce,
                    },
                }),
            )
            .read_only()
            .fetch_from(&env.network)
            .await?
            .data;

        assert_eq!(migrated_transfer.origin_transfer_id, None);
        assert_eq!(
            migrated_transfer.origin_nonce,
            transfer_message.origin_nonce
        );
        assert_eq!(migrated_transfer.recipient, transfer_message.recipient);
        assert_eq!(migrated_transfer.token, transfer_message.token);
        assert_eq!(migrated_transfer.amount, transfer_message.amount);
        assert_eq!(migrated_transfer.fee, transfer_message.fee);
        assert_eq!(migrated_transfer.sender, transfer_message.sender);
        assert_eq!(
            migrated_transfer.destination_nonce,
            transfer_message.destination_nonce
        );

        Ok(())
    }
}
