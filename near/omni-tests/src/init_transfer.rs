#[cfg(test)]
mod tests {
    use crate::helpers::tests::{
        account_1, eth_eoa_address, eth_factory_address, eth_token_address, get_bind_token_args,
        get_claim_fee_args_near, get_event_data, relayer_account_id, LOCKER_PATH, MOCK_PROVER_PATH,
        MOCK_TOKEN_PATH, NEP141_DEPOSIT,
    };
    use anyhow::Ok;
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
    };
    use near_workspaces::{result::ExecutionSuccess, types::NearToken, AccountId};
    use omni_types::{
        near_events::Nep141LockerEvent, ChainKind, Fee, InitTransferMsg, OmniAddress, TransferId,
        TransferMessage, UpdateFee,
    };

    const DEFAULT_NEAR_SANDBOX_BALANCE: NearToken = NearToken::from_near(100);
    const EXPECTED_RELAYER_GAS_COST: NearToken =
        NearToken::from_yoctonear(1_500_000_000_000_000_000_000);

    struct TestEnv {
        worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
        token_contract: near_workspaces::Contract,
        locker_contract: near_workspaces::Contract,
        relayer_account: near_workspaces::Account,
        sender_account: near_workspaces::Account,
        eth_factory_address: OmniAddress,
    }

    impl TestEnv {
        async fn new(sender_balance_token: u128) -> anyhow::Result<Self> {
            let worker = near_workspaces::sandbox().await?;
            // Deploy and initialize FT token
            let token_contract = worker.dev_deploy(&std::fs::read(MOCK_TOKEN_PATH)?).await?;
            token_contract
                .call("new_default_meta")
                .args_json(json!({
                    "owner_id": token_contract.id(),
                    "total_supply": U128(u128::MAX)
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let prover_contract = worker.dev_deploy(&std::fs::read(MOCK_PROVER_PATH)?).await?;
            // Deploy and initialize locker
            let locker_contract = worker.dev_deploy(&std::fs::read(LOCKER_PATH)?).await?;
            locker_contract
                .call("new")
                .args_json(json!({
                    "prover_account": prover_contract.id(),
                    "mpc_signer": "mpc.testnet",
                    "nonce": U128(0),
                    "wnear_account_id": "wnear.testnet",
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Register the locker contract in the token contract
            token_contract
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": locker_contract.id(),
                    "registration_only": true,
                }))
                .deposit(NEP141_DEPOSIT)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Create relayer account. (Default account in sandbox has 100 NEAR)
            let relayer_account = worker
                .create_tla(relayer_account_id(), worker.dev_generate().await.1)
                .await?
                .unwrap();

            // Create sender account. (Default account in sandbox has 100 NEAR)
            let sender_account = worker
                .create_tla(account_1(), worker.dev_generate().await.1)
                .await?
                .unwrap();

            // Register the sender in the token contract
            token_contract
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": sender_account.id(),
                    "registration_only": true,
                }))
                .deposit(NEP141_DEPOSIT)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Register the relayer in the token contract
            token_contract
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": relayer_account.id(),
                    "registration_only": true,
                }))
                .deposit(NEP141_DEPOSIT)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Transfer initial tokens to the sender account
            token_contract
                .call("ft_transfer")
                .args_json(json!({
                    "receiver_id": sender_account.id(),
                    "amount": U128(sender_balance_token),
                    "memo": None::<String>,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Add the ETH factory address to the locker contract
            let eth_factory_address = eth_factory_address();
            locker_contract
                .call("add_factory")
                .args_json(json!({
                    "address": eth_factory_address,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Bind the token to the locker contract
            let required_deposit_for_bind_token = locker_contract
                .view("required_balance_for_bind_token")
                .await?
                .json()?;
            locker_contract
                .call("bind_token")
                .args_borsh(get_bind_token_args(
                    &token_contract.id(),
                    &eth_token_address(),
                    &eth_factory_address,
                ))
                .deposit(required_deposit_for_bind_token)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(Self {
                worker,
                token_contract,
                locker_contract,
                relayer_account,
                sender_account,
                eth_factory_address,
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
                "msg": serde_json::to_string(&init_transfer_msg)?,
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
            transfer_message.origin_nonce.into(),
            env.relayer_account.id().clone(),
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

        let nep141_locker_event: Nep141LockerEvent = serde_json::from_value(
            get_event_data("InitTransferEvent", &logs)?
                .ok_or_else(|| anyhow::anyhow!("InitTransferEvent not found"))?,
        )?;
        let transfer_message = match nep141_locker_event {
            Nep141LockerEvent::InitTransferEvent { transfer_message } => transfer_message,
            _ => anyhow::bail!("InitTransferEvent is found in unexpected event"),
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
                    .checked_sub(transfer_message.fee.native_fee.0)
                    .unwrap_or(0),
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
            get_token_balance(&env.token_contract, &env.sender_account.id()).await?;
        let locker_balance_token: U128 =
            get_token_balance(&env.token_contract, &env.locker_contract.id()).await?;
        let relayer_balance_token: U128 =
            get_token_balance(&env.token_contract, &env.relayer_account.id()).await?;
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
    #[tokio::test]
    async fn test_native_fee() -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 100;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(0),
            recipient: eth_eoa_address(),
        };

        let env = TestEnv::new(sender_balance_token).await?;

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

    #[tokio::test]
    async fn test_transfer_fee() -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(0),
            fee: U128(1000),
            recipient: eth_eoa_address(),
        };

        let env = TestEnv::new(sender_balance_token).await?;

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

    #[tokio::test]
    async fn test_both_fee() -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
        };

        let env = TestEnv::new(sender_balance_token).await?;

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

    #[tokio::test]
    async fn test_update_fee() -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(2).as_yoctonear()),
            fee: U128(2000),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token).await?;

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

    #[tokio::test]
    async fn test_relayer_sign() -> anyhow::Result<()> {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 100;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(0),
            recipient: eth_eoa_address(),
        };

        let env = TestEnv::new(sender_balance_token).await?;

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

    #[tokio::test]
    #[should_panic(expected = "ERR_LOWER_FEE")]
    async fn test_update_fee_native_too_small() {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(0).as_yoctonear()),
            fee: U128(2000),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token).await.unwrap();

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
        ()
    }

    #[tokio::test]
    #[should_panic(expected = "ERR_INVALID_FEE")]
    async fn test_update_fee_transfer_fee_too_small() {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(500),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token).await.unwrap();

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

    #[tokio::test]
    #[should_panic(expected = "ERR_INVALID_FEE")]
    async fn test_update_fee_transfer_fee_too_big() {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
        };
        let update_fee_value = Fee {
            native_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(6000),
        };
        let update_fee = UpdateFee::Fee(update_fee_value.clone());

        let env = TestEnv::new(sender_balance_token).await.unwrap();

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

    #[tokio::test]
    #[should_panic(expected = "TODO")]
    // Add a test once the Proof update fee is implemented
    async fn test_update_fee_proof() {
        let sender_balance_token = 1_000_000;
        let transfer_amount = 5000;
        let init_transfer_msg = InitTransferMsg {
            native_token_fee: U128(NearToken::from_near(1).as_yoctonear()),
            fee: U128(1000),
            recipient: eth_eoa_address(),
        };
        let update_fee = UpdateFee::Proof(vec![]);

        let env = TestEnv::new(sender_balance_token).await.unwrap();

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
}
