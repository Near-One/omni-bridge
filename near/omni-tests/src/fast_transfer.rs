#[cfg(test)]
mod tests {
    use near_sdk::{
        borsh,
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{
        result::{ExecutionResult, Value},
        types::NearToken,
    };
    use omni_types::{
        locker_args::{FinTransferArgs, StorageDepositAction},
        prover_result::{InitTransferMessage, ProverResult},
        BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, Fee, OmniAddress, TransferId,
        TransferMessage,
    };

    use crate::helpers::tests::{
        account_n, base_eoa_address, base_factory_address, eth_eoa_address, eth_factory_address,
        eth_token_address, get_bind_token_args, relayer_account_id, LOCKER_PATH, MOCK_PROVER_PATH,
        MOCK_TOKEN_PATH, NEP141_DEPOSIT,
    };

    struct TestEnv {
        token_contract: near_workspaces::Contract,
        bridge_contract: near_workspaces::Contract,
        relayer_account: near_workspaces::Account,
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
            // Deploy and initialize bridge
            let bridge_contract = worker.dev_deploy(&std::fs::read(LOCKER_PATH)?).await?;
            bridge_contract
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

            // Register the bridge contract in the token contract
            token_contract
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": bridge_contract.id(),
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

            // Transfer initial tokens to the relayer account
            token_contract
                .call("ft_transfer")
                .args_json(json!({
                    "receiver_id": relayer_account.id(),
                    "amount": U128(sender_balance_token),
                    "memo": None::<String>,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Transfer initial tokens to the bridge contract (locked)
            token_contract
                .call("ft_transfer")
                .args_json(json!({
                    "receiver_id": bridge_contract.id(),
                    "amount": U128(sender_balance_token),
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Add ETH factory address to the bridge contract
            let eth_factory_address = eth_factory_address();
            bridge_contract
                .call("add_factory")
                .args_json(json!({
                    "address": eth_factory_address,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Bind the token to the bridge contract
            let required_deposit_for_bind_token = bridge_contract
                .view("required_balance_for_bind_token")
                .await?
                .json()?;
            bridge_contract
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

            // Add base factory address to the bridge contract
            let base_factory_address = base_factory_address();
            bridge_contract
                .call("add_factory")
                .args_json(json!({
                    "address": base_factory_address,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(Self {
                token_contract,
                bridge_contract,
                relayer_account,
            })
        }
    }

    async fn get_balance_required_for_fast_transfer_to_near(
        bridge_contract: &near_workspaces::Contract,
        is_storage_deposit: bool,
    ) -> anyhow::Result<NearToken> {
        let required_balance_for_account: NearToken = bridge_contract
            .view("required_balance_for_account")
            .await?
            .json()?;

        let required_balance_fast_transfer: NearToken = bridge_contract
            .view("required_balance_for_fast_transfer")
            .await?
            .json()?;

        let mut required_balance =
            required_balance_for_account.saturating_add(required_balance_fast_transfer);
        if is_storage_deposit {
            required_balance = required_balance.saturating_add(NEP141_DEPOSIT);
        }

        Ok(required_balance)
    }

    async fn get_balance_required_for_fast_transfer_to_other_chain(
        bridge_contract: &near_workspaces::Contract,
    ) -> anyhow::Result<NearToken> {
        let required_balance_for_account: NearToken = bridge_contract
            .view("required_balance_for_account")
            .await?
            .json()?;

        let required_balance_fast_transfer: NearToken = bridge_contract
            .view("required_balance_for_fast_transfer")
            .await?
            .json()?;

        let required_balance_init_transfer: NearToken = bridge_contract
            .view("required_balance_for_init_transfer")
            .await?
            .json()?;

        Ok(required_balance_for_account
            .saturating_add(required_balance_fast_transfer)
            .saturating_add(required_balance_init_transfer))
    }

    async fn do_fast_transfer(
        env: &TestEnv,
        transfer_amount: u128,
        fast_transfer_msg: FastFinTransferMsg,
    ) -> anyhow::Result<ExecutionResult<Value>> {
        let storage_deposit_amount = match fast_transfer_msg.recipient {
            OmniAddress::Near(_) => {
                get_balance_required_for_fast_transfer_to_near(&env.bridge_contract, true).await?
            }
            _ => {
                get_balance_required_for_fast_transfer_to_other_chain(&env.bridge_contract).await?
            }
        };

        // Deposit to the storage
        env.relayer_account
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({
                "account_id": env.relayer_account.id(),
            }))
            .deposit(storage_deposit_amount)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Initiate the fast transfer
        let transfer_result = env
            .relayer_account
            .call(env.token_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": env.bridge_contract.id(),
                "amount": U128(transfer_amount),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::FastFinTransfer(fast_transfer_msg))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(transfer_result)
    }

    async fn do_fin_transfer(
        env: &TestEnv,
        transfer_msg: InitTransferMessage,
    ) -> anyhow::Result<ExecutionResult<Value>> {
        let required_balance_for_fin_transfer: NearToken = env
            .bridge_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;

        // let required_deposit_for_fin_transfer = NEP141_DEPOSIT
        //     .saturating_add(required_balance_for_fin_transfer);

        let storage_deposit_action = StorageDepositAction {
            token_id: env.token_contract.id().clone(),
            account_id: env.relayer_account.id().clone(),
            storage_deposit_amount: None,
        };

        let result = env
            .relayer_account
            .call(env.bridge_contract.id(), "fin_transfer")
            .args_borsh(FinTransferArgs {
                chain_kind: omni_types::ChainKind::Eth,
                storage_deposit_actions: vec![storage_deposit_action],
                prover_args: borsh::to_vec(&ProverResult::InitTransfer(transfer_msg)).unwrap(),
            })
            .deposit(required_balance_for_fin_transfer)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(result)
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

    #[tokio::test]
    async fn test_fast_transfer_to_near() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_near(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg);

        let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg).await?;

        assert_eq!(0, result.failures().len());

        let recipient_balance: U128 = get_balance(&env.token_contract, &account_n(1)).await?;
        assert_eq!(transfer_amount, recipient_balance.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_fast_transfer_to_near_twice() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_near(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg);

        do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone()).await?;
        let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg).await?;

        assert_eq!(1, result.failures().len());

        let failure = result.failures()[0].clone().into_result();
        assert!(failure.is_err_and(|err| {
            format!("{:?}", err).contains("Fast transfer is already performed")
        }));

        Ok(())
    }

    #[tokio::test]
    async fn test_fast_transfer_to_near_finalisation() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_near(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg.clone());

        do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone()).await?;

        let relayer_balance_before =
            get_balance(&env.token_contract, env.relayer_account.id()).await?;
        let recipient_balance_before = get_balance(&env.token_contract, &account_n(1)).await?;

        do_fin_transfer(&env, transfer_msg).await?;

        let relayer_balance_after =
            get_balance(&env.token_contract, env.relayer_account.id()).await?;
        let recipient_balance_after = get_balance(&env.token_contract, &account_n(1)).await?;

        assert_eq!(
            transfer_amount,
            relayer_balance_after.0 - relayer_balance_before.0
        );
        assert_eq!(recipient_balance_after, recipient_balance_before);

        Ok(())
    }

    #[tokio::test]
    async fn test_fast_transfer_to_near_finalisation_twice() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_near(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg.clone());

        do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone()).await?;

        do_fin_transfer(&env, transfer_msg.clone()).await?;
        let result = do_fin_transfer(&env, transfer_msg).await;

        assert!(result.is_err_and(|err| {
            println!("err: {:?}", err);
            format!("{:?}", err).contains("The transfer is already finalised")
        }));

        Ok(())
    }

    #[tokio::test]
    async fn test_fast_transfer_to_other_chain() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_other_chain(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg.clone());

        let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone()).await?;

        assert_eq!(0, result.failures().len());

        //get_transfer_message
        let transfer_message: TransferMessage = env
            .bridge_contract
            .view("get_transfer_message")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: 1,
                },
            }))
            .await?
            .json()?;

        assert_eq!(
            OmniAddress::Near(env.token_contract.id().clone()),
            transfer_message.token
        );
        assert_eq!(transfer_amount, transfer_message.amount.0);
        assert_eq!(fast_transfer_msg.recipient, transfer_message.recipient);
        assert_eq!(fast_transfer_msg.fee, transfer_message.fee);
        assert_eq!(fast_transfer_msg.msg, transfer_message.msg);
        assert_eq!(
            OmniAddress::Near(env.relayer_account.id().clone()),
            transfer_message.sender
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_fast_transfer_to_other_chain_twice() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_other_chain(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg.clone());

        do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone()).await?;
        let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg).await?;

        assert_eq!(1, result.failures().len());

        let failure = result.failures()[0].clone().into_result();
        assert!(failure.is_err_and(|err| {
            format!("{:?}", err).contains("Fast transfer is already performed")
        }));

        Ok(())
    }

    #[tokio::test]
    async fn test_fast_transfer_to_other_chain_finalisation() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_other_chain(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg.clone());

        do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone()).await?;

        let relayer_balance_before =
            get_balance(&env.token_contract, env.relayer_account.id()).await?;

        do_fin_transfer(&env, transfer_msg).await?;

        let transfer_message = env
            .bridge_contract
            .view("get_transfer_message")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Base,
                    origin_nonce: 0,
                },
            }))
            .await;

        assert!(transfer_message
            .is_err_and(|err| { format!("{:?}", err).contains("The transfer does not exist") }));

        let relayer_balance_after =
            get_balance(&env.token_contract, env.relayer_account.id()).await?;

        assert_eq!(
            transfer_amount,
            relayer_balance_after.0 - relayer_balance_before.0
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_fast_transfer_to_other_chain_finalisation_twice() -> anyhow::Result<()> {
        let env = TestEnv::new(1_000_000).await?;

        let transfer_amount = 100;
        let transfer_msg = get_transfer_msg_to_other_chain(&env, transfer_amount);
        let fast_transfer_msg = get_fast_transfer_msg(transfer_msg.clone());

        do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone()).await?;

        do_fin_transfer(&env, transfer_msg.clone()).await?;
        let result = do_fin_transfer(&env, transfer_msg).await;

        assert!(result.is_err_and(|err| {
            println!("err: {:?}", err);
            format!("{:?}", err).contains("The transfer is already finalised")
        }));

        Ok(())
    }

    fn get_transfer_msg_to_near(env: &TestEnv, amount: u128) -> InitTransferMessage {
        InitTransferMessage {
            origin_nonce: 0,
            token: OmniAddress::Near(env.token_contract.id().clone()),
            recipient: OmniAddress::Near(account_n(1)),
            amount: U128(amount),
            fee: Fee {
                fee: U128(0),
                native_fee: U128(0),
            },
            sender: eth_eoa_address(),
            msg: String::default(),
            emitter_address: eth_factory_address(),
        }
    }

    fn get_transfer_msg_to_other_chain(env: &TestEnv, amount: u128) -> InitTransferMessage {
        InitTransferMessage {
            origin_nonce: 0,
            token: OmniAddress::Near(env.token_contract.id().clone()),
            recipient: eth_eoa_address(),
            amount: U128(amount),
            fee: Fee {
                fee: U128(0),
                native_fee: U128(0),
            },
            sender: base_eoa_address(),
            msg: String::default(),
            emitter_address: base_factory_address(),
        }
    }

    fn get_fast_transfer_msg(transfer_msg: InitTransferMessage) -> FastFinTransferMsg {
        FastFinTransferMsg {
            transfer_id: TransferId {
                origin_chain: transfer_msg.sender.get_chain(),
                origin_nonce: transfer_msg.origin_nonce,
            },
            recipient: transfer_msg.recipient.clone(),
            fee: transfer_msg.fee,
            msg: transfer_msg.msg,
            storage_deposit_amount: match transfer_msg.recipient.get_chain() {
                ChainKind::Near => Some(NEP141_DEPOSIT.as_yoctonear()),
                _ => None,
            },
        }
    }
}
