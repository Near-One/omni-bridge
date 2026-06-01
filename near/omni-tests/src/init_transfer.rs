#[cfg(test)]
mod tests {
    use anyhow::Ok;
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
    };
    use near_workspaces::{result::ExecutionSuccess, types::NearToken, AccountId};
    use omni_types::{
        near_events::OmniBridgeEvent, BoundedString, BridgeOnTransferMsg, ChainKind, Fee,
        InitTransferMsg, OmniAddress, TransferId, TransferMessage, TransferMessageStorageAccount,
        UpdateFee,
    };
    use rstest::rstest;

    use crate::{
        environment::TestEnvBuilder,
        helpers::tests::{
            account_n, build_artifacts, eth_eoa_address, eth_factory_address,
            execution_contains_log, get_claim_fee_args_near, get_event_data, relayer_account_id,
            BuildArtifacts,
        },
    };

    const DEFAULT_NEAR_SANDBOX_BALANCE: NearToken = NearToken::from_near(100);
    const EXPECTED_RELAYER_GAS_COST: NearToken =
        NearToken::from_yoctonear(5_000_000_000_000_000_000_000);

    struct TestEnv {
        worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
        token_contract: near_workspaces::Contract,
        locker_contract: near_workspaces::Contract,
        relayer_account: near_workspaces::Account,
        sender_account: near_workspaces::Account,
        eth_factory_address: OmniAddress,
    }

    impl TestEnv {
        #[allow(clippy::too_many_lines)]
        async fn new(
            sender_balance_token: u128,
            is_old_locker: bool,
            build_artifacts: &BuildArtifacts,
        ) -> anyhow::Result<Self> {
            let env_builder = TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .deploy_old_version(is_old_locker)
                .with_native_nep141_token(24)
                .await?;
            let relayer_account = if is_old_locker {
                env_builder.create_account(relayer_account_id()).await?
            } else {
                env_builder
                    .setup_trusted_relayer(relayer_account_id())
                    .await?
            };
            let sender_account = env_builder.create_account(account_n(1)).await?;

            env_builder.storage_deposit(relayer_account.id()).await?;
            env_builder.storage_deposit(sender_account.id()).await?;
            env_builder
                .mint_tokens(sender_account.id(), sender_balance_token)
                .await?;

            Ok(Self {
                worker: env_builder.worker,
                token_contract: env_builder.token.contract,
                locker_contract: env_builder.bridge_contract,
                relayer_account,
                sender_account,
                eth_factory_address: eth_factory_address(),
            })
        }
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
        )
        .await?;

        // Deposit to the storage
        env.sender_account
            .call(env.locker_contract.id(), "storage_deposit")
            .args_json(json!({
                "account_id": env.sender_account.id(),
            }))
            .deposit(storage_deposit_amount)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Initiate the transfer
        let transfer_result = env
            .sender_account
            .call(env.token_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": env.locker_contract.id(),
                "amount": U128(transfer_amount),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(init_transfer_msg))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Ensure the transfer event is emitted
        let transfer_message = get_transfer_message_from_event(&transfer_result)?;

        // Update the transfer fee if needed
        let signing_fee = if let Some(update_fee) = update_fee.clone() {
            make_fee_update(
                update_fee.clone(),
                &transfer_message,
                &env.locker_contract,
                &env.sender_account,
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

        signer
            .call(env.locker_contract.id(), "sign_transfer")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: transfer_message.origin_nonce,
                },
                "fee_recipient": env.relayer_account.id(),
                "fee": &Some(signing_fee.clone()),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Relayer claims the fee
        let claim_fee_args = get_claim_fee_args_near(
            ChainKind::Near,
            ChainKind::Eth,
            transfer_message.origin_nonce,
            env.relayer_account.id(),
            transfer_amount - signing_fee.fee.0,
            env.eth_factory_address.clone(),
        );
        env.relayer_account
            .call(env.locker_contract.id(), "claim_fee")
            .args_borsh(claim_fee_args)
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }
    async fn get_balance_required_for_account(
        locker_contract: &near_workspaces::Contract,
        sender_account: &near_workspaces::Account,
        init_transfer_msg: &InitTransferMsg,
        custom_deposit: Option<NearToken>,
    ) -> anyhow::Result<NearToken> {
        let required_balance_account: NearToken = locker_contract
            .view("required_balance_for_account")
            .await?
            .json()?;

        let required_balance_init_transfer: NearToken = locker_contract
            .view("required_balance_for_init_transfer")
            .args_json(json!({
                "recipient": init_transfer_msg.recipient,
                "sender": OmniAddress::Near(sender_account.id().clone()),
            }))
            .await?
            .json()?;

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
        transfer_result: &ExecutionSuccess,
    ) -> anyhow::Result<TransferMessage> {
        let logs = transfer_result
            .receipt_outcomes()
            .iter()
            .flat_map(|outcome| &outcome.logs)
            .collect::<Vec<_>>();

        let omni_bridge_event: OmniBridgeEvent = serde_json::from_value(
            get_event_data("InitTransferEvent", &logs)?
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
        locker_contract: &near_workspaces::Contract,
        sender_account: &near_workspaces::Account,
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
        sender_account
            .call(locker_contract.id(), "update_transfer_fee")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: transfer_message.origin_nonce,
                },
                "fee": update_fee.clone(),
            }))
            .max_gas()
            .deposit(deposit)
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    async fn get_token_balance(
        token_contract: &near_workspaces::Contract,
        account_id: &AccountId,
    ) -> anyhow::Result<U128> {
        Ok(token_contract
            .view("ft_balance_of")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?)
    }
    async fn get_test_balances(env: &TestEnv) -> anyhow::Result<(U128, U128, U128, NearToken)> {
        let user_balance_token: U128 =
            get_token_balance(&env.token_contract, env.sender_account.id()).await?;
        let locker_balance_token: U128 =
            get_token_balance(&env.token_contract, env.locker_contract.id()).await?;
        let relayer_balance_token: U128 =
            get_token_balance(&env.token_contract, env.relayer_account.id()).await?;
        let relayer_balance_near: NearToken = env
            .worker
            .view_account(env.relayer_account.id())
            .await?
            .balance;

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
            external_id: None,
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
    async fn test_transfer_fee(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(0),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
            external_id: None,
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
            external_id: None,
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
            external_id: None,
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
            true,
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
            external_id: None,
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

    /// Exercises the yield/resume path of `init_transfer` and confirms that `external_id`
    /// is threaded into the virtual storage account ID:
    /// 1. Sender registers with the bridge at the bare minimum (no transfer-sized storage).
    /// 2. Two `ft_transfer_call`s are submitted with otherwise-identical messages but
    ///    different `external_id`s. Both yield because the signer lacks storage and the
    ///    virtual accounts don't exist yet.
    /// 3. Two `storage_deposit`s — one per predicted virtual account ID — resume each
    ///    yielded transfer independently.
    /// 4. Each resumed transfer is then signed and its fee claimed as usual.
    #[rstest]
    #[tokio::test]
    async fn test_init_transfer_with_external_id(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let fee = U128(1000);

        let env = TestEnv::new(sender_balance_token, false, build_artifacts).await?;

        // Register the sender with the bare-minimum bridge storage. This is enough to
        // identify the signer during resume, but not enough to fund the transfer — so
        // `init_transfer` must take the yield branch.
        let required_balance_for_account: NearToken = env
            .locker_contract
            .view("required_balance_for_account")
            .await?
            .json()?;
        env.sender_account
            .call(env.locker_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": env.sender_account.id() }))
            .deposit(required_balance_for_account)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // The yield branch charges the bridge contract's own storage balance (for the
        // init_transfer_promises entry). Fund it so `init_transfer` can register the yield.
        env.sender_account
            .call(env.locker_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": env.locker_contract.id() }))
            .deposit(NearToken::from_millinear(100))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Build two InitTransferMsgs that differ only by external_id.
        let msg_a = InitTransferMsg {
            native_token_fee: U128(0),
            fee,
            recipient: eth_eoa_address(),
            msg: None,
            external_id: Some(BoundedString::new("external-id-a").unwrap()),
        };
        let msg_b = InitTransferMsg {
            external_id: Some(BoundedString::new("external-id-b").unwrap()),
            ..msg_a.clone()
        };

        // Predict the virtual storage account each transfer will yield on. All the fields
        // consumed by the hash are known in advance: the chain-side `TransferMessage` only
        // adds nonces to this struct, and nonces are not part of the storage-account hash.
        let storage_account = TransferMessageStorageAccount {
            token: OmniAddress::Near(env.token_contract.id().clone()),
            amount: U128(transfer_amount),
            recipient: msg_a.recipient.clone(),
            fee: Fee {
                fee: msg_a.fee,
                native_fee: msg_a.native_token_fee,
            },
            sender: OmniAddress::Near(env.sender_account.id().clone()),
            msg: String::new(),
        };
        let virtual_a = storage_account.id(Some("external-id-a".to_string()));
        let virtual_b = storage_account.id(Some("external-id-b".to_string()));
        assert_ne!(
            virtual_a, virtual_b,
            "Different external_ids must yield different virtual storage accounts"
        );

        // Fire both ft_transfer_calls without awaiting final execution — each one yields
        // inside ft_on_transfer, so the tx stays in "Started" state until resumed.
        let submit_ft_transfer_call = |msg: InitTransferMsg| {
            env.sender_account
                .call(env.token_contract.id(), "ft_transfer_call")
                .args_json(json!({
                    "receiver_id": env.locker_contract.id(),
                    "amount": U128(transfer_amount),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(msg))
                        .unwrap(),
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact_async()
        };
        let status_a = submit_ft_transfer_call(msg_a.clone()).await?;
        let status_b = submit_ft_transfer_call(msg_b.clone()).await?;

        // Fast-forward a few blocks so both yield/resume callbacks are processed.
        env.worker.fast_forward(5).await?;

        let required_balance_init_transfer: NearToken = env
            .locker_contract
            .view("required_balance_for_init_transfer")
            .args_json(json!({}))
            .await?
            .json()?;

        // Depositing storage on each virtual account resumes its yielded transfer. The
        // yield callback (`init_transfer_resume`) runs as part of the original
        // ft_transfer_call's receipt chain, so the `InitTransferEvent` shows up there.
        for virtual_id in [&virtual_a, &virtual_b] {
            env.relayer_account
                .call(env.locker_contract.id(), "storage_deposit")
                .args_json(json!({ "account_id": virtual_id }))
                .deposit(required_balance_init_transfer)
                .max_gas()
                .transact()
                .await?
                .into_result()?;
        }

        // Each ft_transfer_call should now unblock — resumed by its storage_deposit.
        let result_a = status_a.await?.into_result()?;
        let result_b = status_b.await?.into_result()?;

        for result in [&result_a, &result_b] {
            assert!(
                execution_contains_log(result, "Yield init transfer until storage is available at"),
                "Expected yield log not found in receipt outcomes"
            );
        }

        // Sign + claim fee for each transfer.
        for result in [&result_a, &result_b] {
            let transfer_message = get_transfer_message_from_event(result)?;
            env.relayer_account
                .call(env.locker_contract.id(), "sign_transfer")
                .args_json(json!({
                    "transfer_id": TransferId {
                        origin_chain: ChainKind::Near,
                        origin_nonce: transfer_message.origin_nonce,
                    },
                    "fee_recipient": env.relayer_account.id(),
                    "fee": &Some(transfer_message.fee.clone()),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let claim_fee_args = get_claim_fee_args_near(
                ChainKind::Near,
                ChainKind::Eth,
                transfer_message.origin_nonce,
                env.relayer_account.id(),
                transfer_amount - transfer_message.fee.fee.0,
                env.eth_factory_address.clone(),
            );
            env.relayer_account
                .call(env.locker_contract.id(), "claim_fee")
                .args_borsh(claim_fee_args)
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;
        }

        let (user_balance_token, locker_balance_token, relayer_balance_token, _) =
            get_test_balances(&env).await?;
        assert_eq!(
            user_balance_token,
            U128(sender_balance_token - 2 * transfer_amount),
            "User balance was not deducted for both resumed transfers"
        );
        assert_eq!(
            locker_balance_token,
            U128(2 * (transfer_amount - fee.0)),
            "Locker balance was not increased for both resumed transfers"
        );
        assert_eq!(
            relayer_balance_token,
            U128(2 * fee.0),
            "Relayer didn't receive transfer fee for both resumed transfers"
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_untrusted_sender_cannot_sign_transfer(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 100;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(0),
            fee: U128(0),
            recipient: eth_eoa_address(),
            msg: None,
            external_id: None,
        };

        let env = TestEnv::new(sender_balance_token, false, build_artifacts).await?;

        let storage_deposit_amount = get_balance_required_for_account(
            &env.locker_contract,
            &env.sender_account,
            &init_transfer_msg,
            None,
        )
        .await?;

        env.sender_account
            .call(env.locker_contract.id(), "storage_deposit")
            .args_json(json!({
                "account_id": env.sender_account.id(),
            }))
            .deposit(storage_deposit_amount)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let transfer_result = env
            .sender_account
            .call(env.token_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": env.locker_contract.id(),
                "amount": U128(transfer_amount),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(init_transfer_msg))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let transfer_message = get_transfer_message_from_event(&transfer_result)?;

        // sender_account is not a trusted relayer, so sign_transfer should fail
        let result = env
            .sender_account
            .call(env.locker_contract.id(), "sign_transfer")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: transfer_message.origin_nonce,
                },
                "fee_recipient": env.relayer_account.id(),
                "fee": &Some(transfer_message.fee.clone()),
            }))
            .max_gas()
            .transact()
            .await?;

        assert!(
            result.into_result().is_err(),
            "Unprivileged sender should not be able to call sign_transfer"
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
            external_id: None,
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
            external_id: None,
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
            external_id: None,
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
    #[should_panic(expected = "ERR_UNSUPPORTED_FEE_UPDATE_PROOF")]
    // Add a test once the Proof update fee is implemented
    async fn test_update_fee_proof(build_artifacts: &BuildArtifacts) {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
            msg: None,
            external_id: None,
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
        let env = TestEnv::new(sender_balance_token, true, build_artifacts).await?;

        let res = env
            .locker_contract
            .as_account()
            .deploy(&build_artifacts.locker)
            .await
            .unwrap();

        assert!(res.is_success(), "Failed to upgrade locker");

        let res = env
            .locker_contract
            .call("migrate")
            .max_gas()
            .transact()
            .await?;

        assert!(res.is_success(), "Migration didn't succeed");

        let transfer = env
            .locker_contract
            .call("get_locked_tokens")
            .args_json(json!({
                "chain_kind": ChainKind::Near,
                "token_id": env.token_contract.id(),
            }))
            .max_gas()
            .transact()
            .await?;

        assert_eq!(
            transfer
                .into_result()?
                .json::<Option<U128>>()?
                .unwrap_or(U128(0)),
            U128(0),
            "Locked tokens should be empty after migration"
        );

        Ok(())
    }
}
