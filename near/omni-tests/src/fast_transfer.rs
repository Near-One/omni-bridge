#[cfg(test)]
mod tests {
    use std::str::FromStr;

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
        BasicMetadata, BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, Fee, OmniAddress,
        TransferId, TransferMessage,
    };

    use crate::helpers::tests::{
        account_n, base_eoa_address, base_factory_address, eth_eoa_address, eth_factory_address,
        eth_token_address, fast_relayer_account_id, get_bind_token_args, locker_wasm,
        mock_prover_wasm, mock_token_wasm, relayer_account_id, token_deployer_wasm, NEP141_DEPOSIT,
    };

    struct TestEnv {
        token_contract: near_workspaces::Contract,
        eth_token_address: OmniAddress,
        locker_contract: near_workspaces::Contract,
        relayer_account: near_workspaces::Account,
        fast_relayer_account: near_workspaces::Account,
    }

    impl TestEnv {
        async fn new_with_native_token() -> anyhow::Result<Self> {
            Self::new(false).await
        }

        async fn new_with_bridged_token() -> anyhow::Result<Self> {
            Self::new(true).await
        }

        #[allow(clippy::too_many_lines)]
        async fn new(is_bridged_token: bool) -> anyhow::Result<Self> {
            let sender_balance_token = 1_000_000_000_000;
            let worker = near_workspaces::sandbox().await?;

            // Deploy and initialize bridge
            let locker_contract = worker.dev_deploy(&locker_wasm()).await?;
            locker_contract
                .call("new")
                .args_json(json!({
                    "mpc_signer": "mpc.testnet",
                    "nonce": U128(0),
                    "wnear_account_id": "wnear.testnet",
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let prover = worker.dev_deploy(&mock_prover_wasm()).await?;
            locker_contract
                .call("add_prover")
                .args_json(json!({
                    "chain": "Eth",
                    "account_id": prover.id(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Add ETH factory address to the bridge contract
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

            let base_factory_address = base_factory_address();
            locker_contract
                .call("add_factory")
                .args_json(json!({
                    "address": base_factory_address,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let token_deployer = worker
                .create_tla_and_deploy(
                    account_n(1),
                    worker.dev_generate().await.1,
                    &token_deployer_wasm(),
                )
                .await?
                .unwrap();

            token_deployer
                .call("new")
                .args_json(json!({
                    "controller": locker_contract.id(),
                    "dao": AccountId::from_str("dao.near").unwrap(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            locker_contract
                .call("add_token_deployer")
                .args_json(json!({
                    "chain": eth_factory_address.get_chain(),
                    "account_id": token_deployer.id(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Create relayer accounts. (Default account in sandbox has 100 NEAR)
            let relayer_account = worker
                .create_tla(relayer_account_id(), worker.dev_generate().await.1)
                .await?
                .unwrap();
            let fast_relayer_account = worker
                .create_tla(fast_relayer_account_id(), worker.dev_generate().await.1)
                .await?
                .unwrap();

            let (token_contract, eth_token_address) = if is_bridged_token {
                let (token_contract, eth_token_address) =
                    Self::deploy_bridged_token(&worker, &locker_contract).await?;

                // Mint to relayer account
                Self::fake_finalize_transfer(
                    &locker_contract,
                    &token_contract,
                    eth_token_address.clone(),
                    &relayer_account,
                    eth_factory_address.clone(),
                    U128(sender_balance_token),
                    1,
                )
                .await?;

                // Mint to fast relayer account
                Self::fake_finalize_transfer(
                    &locker_contract,
                    &token_contract,
                    eth_token_address.clone(),
                    &fast_relayer_account,
                    eth_factory_address,
                    U128(sender_balance_token * 2),
                    2,
                )
                .await?;

                // Register the bridge in the token contract
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

                (token_contract, eth_token_address)
            } else {
                let (token_contract, eth_token_address) =
                    Self::deploy_native_token(worker, &locker_contract, eth_factory_address)
                        .await?;

                // Register and send tokens to the relayer account
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

                // Register and send tokens to the fast relayer account
                token_contract
                    .call("storage_deposit")
                    .args_json(json!({
                        "account_id": fast_relayer_account.id(),
                        "registration_only": true,
                    }))
                    .deposit(NEP141_DEPOSIT)
                    .max_gas()
                    .transact()
                    .await?
                    .into_result()?;

                token_contract
                    .call("ft_transfer")
                    .args_json(json!({
                        "receiver_id": fast_relayer_account.id(),
                        "amount": U128(sender_balance_token * 2),
                        "memo": None::<String>,
                    }))
                    .deposit(NearToken::from_yoctonear(1))
                    .max_gas()
                    .transact()
                    .await?
                    .into_result()?;

                // Register and send tokens to the bridge contract
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

                token_contract
                    .call("ft_transfer")
                    .args_json(json!({
                        "receiver_id": locker_contract.id(),
                        "amount": U128(sender_balance_token),
                        "memo": None::<String>,
                    }))
                    .deposit(NearToken::from_yoctonear(1))
                    .max_gas()
                    .transact()
                    .await?
                    .into_result()?;

                (token_contract, eth_token_address)
            };

            // Transfer tokens to the bridge contract to test that exist balances don't affect the fast transfer
            relayer_account
                .call(token_contract.id(), "ft_transfer")
                .args_json(json!({
                    "receiver_id": locker_contract.id(),
                    "amount": U128(100_000_000),
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(Self {
                token_contract,
                eth_token_address,
                locker_contract,
                relayer_account,
                fast_relayer_account,
            })
        }

        async fn deploy_bridged_token(
            worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
            bridge_contract: &near_workspaces::Contract,
        ) -> anyhow::Result<(near_workspaces::Contract, OmniAddress)> {
            let init_token_address = OmniAddress::new_zero(ChainKind::Eth).unwrap();
            let token_metadata = BasicMetadata {
                name: "ETH from Ethereum".to_string(),
                symbol: "ETH".to_string(),
                decimals: 18,
            };

            let required_storage: NearToken = bridge_contract
                .view("required_balance_for_deploy_token")
                .await?
                .json()?;

            bridge_contract
                .call("deploy_native_token")
                .args_json(json!({
                    "chain_kind": init_token_address.get_chain(),
                    "name": token_metadata.name,
                    "symbol": token_metadata.symbol,
                    "decimals": token_metadata.decimals,
                }))
                .deposit(required_storage)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let token_account_id: AccountId = bridge_contract
                .view("get_token_id")
                .args_json(json!({
                    "address": init_token_address
                }))
                .await?
                .json()?;

            let token_contract = worker
                .import_contract(&token_account_id, worker)
                .transact()
                .await?;

            Ok((token_contract, init_token_address))
        }

        async fn deploy_native_token(
            worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
            bridge_contract: &near_workspaces::Contract,
            eth_factory_address: OmniAddress,
        ) -> Result<(near_workspaces::Contract, OmniAddress), anyhow::Error> {
            let token_contract = worker.dev_deploy(&mock_token_wasm()).await?;
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
            let required_deposit_for_bind_token = bridge_contract
                .view("required_balance_for_bind_token")
                .await?
                .json()?;

            bridge_contract
                .call("bind_token")
                .args_borsh(get_bind_token_args(
                    token_contract.id(),
                    &eth_token_address(),
                    &eth_factory_address,
                    18,
                    24,
                ))
                .deposit(required_deposit_for_bind_token)
                .max_gas()
                .transact()
                .await?
                .into_result()?;
            Ok((token_contract, eth_token_address()))
        }

        async fn fake_finalize_transfer(
            bridge_contract: &near_workspaces::Contract,
            token_contract: &near_workspaces::Contract,
            eth_token_address: OmniAddress,
            recipient: &near_workspaces::Account,
            emitter_address: OmniAddress,
            amount: U128,
            nonce: u64,
        ) -> anyhow::Result<()> {
            let storage_deposit_actions = vec![StorageDepositAction {
                token_id: token_contract.id().clone(),
                account_id: recipient.id().clone(),
                storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
            }];
            let required_balance_for_fin_transfer: NearToken = bridge_contract
                .view("required_balance_for_fin_transfer")
                .await?
                .json()?;
            let required_deposit_for_fin_transfer =
                NEP141_DEPOSIT.saturating_add(required_balance_for_fin_transfer);

            // Simulate finalization of transfer through locker
            bridge_contract
                .call("fin_transfer")
                .args_borsh(FinTransferArgs {
                    chain_kind: ChainKind::Eth,
                    storage_deposit_actions,
                    prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                        origin_nonce: nonce,
                        token: eth_token_address,
                        recipient: OmniAddress::Near(recipient.id().clone()),
                        amount,
                        fee: Fee {
                            fee: U128(0),
                            native_fee: U128(0),
                        },
                        sender: eth_eoa_address(),
                        msg: String::default(),
                        emitter_address,
                    }))?,
                })
                .deposit(required_deposit_for_fin_transfer)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(())
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
        relayer_account: Option<&near_workspaces::Account>,
    ) -> anyhow::Result<ExecutionResult<Value>> {
        let relayer_account = relayer_account.unwrap_or(&env.relayer_account);

        let storage_deposit_amount = match fast_transfer_msg.recipient {
            OmniAddress::Near(_) => {
                get_balance_required_for_fast_transfer_to_near(&env.locker_contract, true).await?
            }
            _ => {
                get_balance_required_for_fast_transfer_to_other_chain(&env.locker_contract).await?
            }
        };

        // Deposit to the storage
        relayer_account
            .call(env.locker_contract.id(), "storage_deposit")
            .args_json(json!({
                "account_id": relayer_account.id(),
            }))
            .deposit(storage_deposit_amount)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Initiate the fast transfer
        let transfer_result = relayer_account
            .call(env.token_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": env.locker_contract.id(),
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
        fast_relayer_account: Option<&near_workspaces::Account>,
    ) -> anyhow::Result<ExecutionResult<Value>> {
        let fast_relayer_account = fast_relayer_account.unwrap_or(&env.relayer_account);

        let required_balance_for_fin_transfer: NearToken = env
            .locker_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;

        let required_balance_for_init_transfer: NearToken = env
            .locker_contract
            .view("required_balance_for_init_transfer")
            .await?
            .json()?;

        let attached_deposit = required_balance_for_init_transfer
            .saturating_add(required_balance_for_fin_transfer)
            .saturating_add(NEP141_DEPOSIT);

        let storage_deposit_action = StorageDepositAction {
            token_id: env.token_contract.id().clone(),
            account_id: fast_relayer_account.id().clone(),
            storage_deposit_amount: None,
        };

        let result = env
            .relayer_account
            .call(env.locker_contract.id(), "fin_transfer")
            .args_borsh(FinTransferArgs {
                chain_kind: omni_types::ChainKind::Eth,
                storage_deposit_actions: vec![
                    storage_deposit_action.clone(),
                    storage_deposit_action,
                ],
                prover_args: borsh::to_vec(&ProverResult::InitTransfer(transfer_msg)).unwrap(),
            })
            .deposit(attached_deposit)
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

    mod transfer_to_near {
        use super::*;

        #[tokio::test]
        async fn succeeds_with_native_token() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let fee = 1_000_000;
            let decimal_diff = 6;
            let (_, fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, fee, decimal_diff);

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            assert_eq!(0, result.failures().len());

            let recipient_balance: U128 = get_balance(&env.token_contract, &account_n(1)).await?;
            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(transfer_amount, recipient_balance.0);
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + transfer_amount)
            );

            Ok(())
        }

        #[tokio::test]
        async fn succeeds_with_bridged_token() -> anyhow::Result<()> {
            let env = TestEnv::new_with_bridged_token().await?;

            let transfer_amount = 100_000_000;
            let (_, fast_transfer_msg) = get_transfer_to_near_msg(&env, transfer_amount, 0, 0);

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(U128(transfer_amount), contract_balance_before);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            assert_eq!(0, result.failures().len());

            let recipient_balance: U128 = get_balance(&env.token_contract, &account_n(1)).await?;
            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(transfer_amount, recipient_balance.0);
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + transfer_amount)
            );

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_invalid_amount() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let fee = 1_000_000;
            let decimal_diff = 6;
            let (_, mut fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, fee, decimal_diff);
            fast_transfer_msg.amount = U128(100_000_000);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            assert_eq!(1, result.failures().len());
            let failure = result.failures()[0].clone().into_result();
            assert!(failure.is_err_and(|err| {
                format!("{err:?}").contains("ERR_INVALID_FAST_TRANSFER_AMOUNT")
            }));

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_invalid_fee() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let fee = 1_000_000;
            let decimal_diff = 6;
            let (_, mut fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, fee, decimal_diff);
            fast_transfer_msg.fee.fee = U128(2);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            assert_eq!(1, result.failures().len());
            let failure = result.failures()[0].clone().into_result();
            assert!(failure.is_err_and(|err| {
                format!("{err:?}").contains("ERR_INVALID_FAST_TRANSFER_AMOUNT")
            }));

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_bad_storage_deposit() -> anyhow::Result<()> {
            let env = TestEnv::new_with_bridged_token().await?;

            let transfer_amount = 100_000_000;
            let (_, mut fast_transfer_msg) = get_transfer_to_near_msg(&env, transfer_amount, 0, 0);

            fast_transfer_msg.storage_deposit_amount =
                Some(U128(NEP141_DEPOSIT.saturating_mul(100).as_yoctonear()));

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(U128(transfer_amount), contract_balance_before);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            assert_eq!(1, result.failures().len());
            let failure = result.failures()[0].clone().into_result();
            assert!(failure
                .is_err_and(|err| { format!("{err:?}").contains("Not enough storage deposited") }));

            let recipient_balance: U128 = get_balance(&env.token_contract, &account_n(1)).await?;
            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(0, recipient_balance.0);
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(relayer_balance_before, relayer_balance_after);

            Ok(())
        }

        #[tokio::test]
        async fn succeeds_with_non_duplicate_transfer() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let decimal_diff = 6;
            let (_, fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, 0, decimal_diff);

            do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            let OmniAddress::Near(recipient) = fast_transfer_msg.recipient.clone() else {
                panic!("Recipient is not a Near address");
            };

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;
            let recipient_balance_before = get_balance(&env.token_contract, &recipient).await?;

            let transfer_amount = transfer_amount + 10_000_000;
            let decimal_diff = 6;
            let (_, fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, 0, decimal_diff);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            assert_eq!(0, result.failures().len());

            let recipient_balance: U128 = get_balance(&env.token_contract, &account_n(1)).await?;
            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(
                recipient_balance_before.0 + transfer_amount,
                recipient_balance.0
            );
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + transfer_amount)
            );

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_duplicate_transfer() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let decimal_diff = 6;
            let (_, fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, 0, decimal_diff);

            do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            let OmniAddress::Near(recipient) = fast_transfer_msg.recipient.clone() else {
                panic!("Recipient is not a Near address");
            };

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;
            let recipient_balance_before = get_balance(&env.token_contract, &recipient).await?;

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;
            assert_eq!(1, result.failures().len());

            let failure = result.failures()[0].clone().into_result();
            assert!(failure.is_err_and(|err| {
                format!("{err:?}").contains("Fast transfer is already performed")
            }));

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;
            let recipient_balance_after = get_balance(&env.token_contract, &recipient).await?;

            assert_eq!(relayer_balance_before, relayer_balance_after);
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(recipient_balance_before, recipient_balance_after);

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_duplicate_transfer_with_bridged_token() -> anyhow::Result<()> {
            let env = TestEnv::new_with_bridged_token().await?;

            let transfer_amount = 100_000_000;
            let (_, fast_transfer_msg) = get_transfer_to_near_msg(&env, transfer_amount, 0, 0);

            do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            let OmniAddress::Near(recipient) = fast_transfer_msg.recipient.clone() else {
                panic!("Recipient is not a Near address");
            };

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;
            let recipient_balance_before = get_balance(&env.token_contract, &recipient).await?;

            assert_eq!(U128(transfer_amount), contract_balance_before);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;
            assert!(!result.failures().is_empty());

            let failure = result.failures()[0].clone().into_result();
            assert!(failure.is_err_and(|err| {
                format!("{err:?}").contains("Fast transfer is already performed")
            }));

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;
            let recipient_balance_after = get_balance(&env.token_contract, &recipient).await?;

            assert_eq!(relayer_balance_before, relayer_balance_after);
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(recipient_balance_before, recipient_balance_after);

            Ok(())
        }
    }

    mod finalisation_to_near {
        use super::*;

        #[tokio::test]
        async fn succeeds() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let fee = 1_000_000;
            let decimal_diff = 6;
            let (transfer_msg, mut fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, fee, decimal_diff);
            fast_transfer_msg.relayer = env.fast_relayer_account.id().clone();

            do_fast_transfer(
                &env,
                transfer_amount,
                fast_transfer_msg.clone(),
                Some(&env.fast_relayer_account),
            )
            .await?;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let fast_relayer_balance_before =
                get_balance(&env.token_contract, env.fast_relayer_account.id()).await?;
            let recipient_balance_before = get_balance(&env.token_contract, &account_n(1)).await?;

            do_fin_transfer(&env, transfer_msg, Some(&env.fast_relayer_account)).await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let fast_relayer_balance_after =
                get_balance(&env.token_contract, env.fast_relayer_account.id()).await?;
            let recipient_balance_after = get_balance(&env.token_contract, &account_n(1)).await?;

            assert_eq!(
                transfer_amount + fee,
                fast_relayer_balance_after.0 - fast_relayer_balance_before.0
            );
            assert_eq!(recipient_balance_after, recipient_balance_before);
            assert_eq!(relayer_balance_after, relayer_balance_before);

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_duplicate_finalisation() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let decimal_diff = 6;
            let (transfer_msg, fast_transfer_msg) =
                get_transfer_to_near_msg(&env, transfer_amount, 0, decimal_diff);

            do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            do_fin_transfer(&env, transfer_msg.clone(), None).await?;
            let result = do_fin_transfer(&env, transfer_msg, None).await;

            assert!(result.is_err_and(|err| {
                format!("{err:?}").contains("The transfer is already finalised")
            }));

            Ok(())
        }
    }

    mod transfer_to_other_chain {
        use super::*;

        #[tokio::test]
        async fn succeeds_with_native_token() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let fee = 1_000_000;
            let decimal_diff = 6;
            let (_, fast_transfer_msg) =
                get_transfer_to_other_chain_msg(&env, transfer_amount, fee, decimal_diff);

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            let result =
                do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            assert_eq!(0, result.failures().len());

            //get_transfer_message
            let transfer_message: TransferMessage = env
                .locker_contract
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
            assert_eq!(transfer_amount + fee, transfer_message.amount.0);
            assert_eq!(fast_transfer_msg.recipient, transfer_message.recipient);
            assert_eq!(
                fast_transfer_msg.fee.native_fee,
                transfer_message.fee.native_fee
            );
            assert_eq!(fee, transfer_message.fee.fee.0);
            assert_eq!(fast_transfer_msg.msg, transfer_message.msg);
            assert_eq!(
                OmniAddress::Near(env.relayer_account.id().clone()),
                transfer_message.sender
            );

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(
                contract_balance_before,
                U128(contract_balance_after.0 - transfer_amount)
            );
            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + transfer_amount)
            );

            Ok(())
        }

        #[tokio::test]
        async fn succeeds_with_bridged_token() -> anyhow::Result<()> {
            let env = TestEnv::new_with_bridged_token().await?;

            let transfer_amount = 100_000_000;
            let (_, fast_transfer_msg) =
                get_transfer_to_other_chain_msg(&env, transfer_amount, 0, 0);

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(U128(transfer_amount), contract_balance_before);

            let result =
                do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            assert_eq!(0, result.failures().len());

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + transfer_amount)
            );

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_duplicate_transfer() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let decimal_diff = 6;
            let (_, fast_transfer_msg) =
                get_transfer_to_other_chain_msg(&env, transfer_amount, 0, decimal_diff);

            do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            assert_eq!(1, result.failures().len());

            let failure = result.failures()[0].clone().into_result();
            assert!(failure.is_err_and(|err| {
                format!("{err:?}").contains("Fast transfer is already performed")
            }));

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(relayer_balance_before, relayer_balance_after);
            assert_eq!(contract_balance_before, contract_balance_after);

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_already_finalised() -> anyhow::Result<()> {
            let env = TestEnv::new_with_bridged_token().await?;

            let transfer_amount = 100_000_000;
            let (transfer_msg, fast_transfer_msg) =
                get_transfer_to_other_chain_msg(&env, transfer_amount, 0, 0);

            do_fin_transfer(&env, transfer_msg, None).await?;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(U128(transfer_amount), contract_balance_before);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.locker_contract.id()).await?;

            assert_eq!(relayer_balance_before, relayer_balance_after);
            assert_eq!(contract_balance_before, contract_balance_after);

            assert_eq!(1, result.failures().len());
            let failure = result.failures()[0].clone().into_result();
            assert!(failure.is_err_and(|err| {
                format!("{err:?}").contains("ERR_TRANSFER_ALREADY_FINALISED")
            }));

            Ok(())
        }
    }

    mod finalisation_to_other_chain {
        use super::*;

        #[tokio::test]
        async fn succeeds() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let decimal_diff = 6;
            let (transfer_msg, fast_transfer_msg) =
                get_transfer_to_other_chain_msg(&env, transfer_amount, 0, decimal_diff);

            do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;

            do_fin_transfer(&env, transfer_msg, None).await?;

            let transfer_message = env
                .locker_contract
                .view("get_transfer_message")
                .args_json(json!({
                    "transfer_id": TransferId {
                        origin_chain: ChainKind::Base,
                        origin_nonce: 0,
                    },
                }))
                .await;

            assert!(transfer_message
                .is_err_and(|err| { format!("{err:?}").contains("The transfer does not exist") }));

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;

            assert_eq!(
                transfer_amount,
                relayer_balance_after.0 - relayer_balance_before.0
            );

            Ok(())
        }

        #[tokio::test]
        async fn fails_due_to_duplicate_finalisation() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let transfer_amount = 100_000_000;
            let decimal_diff = 6;
            let (transfer_msg, fast_transfer_msg) =
                get_transfer_to_other_chain_msg(&env, transfer_amount, 0, decimal_diff);

            do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;

            do_fin_transfer(&env, transfer_msg.clone(), None).await?;
            let result = do_fin_transfer(&env, transfer_msg, None).await;

            assert!(result.is_err_and(|err| {
                format!("{err:?}").contains("The transfer is already finalised")
            }));

            Ok(())
        }
    }

    fn get_transfer_to_near_msg(
        env: &TestEnv,
        amount: u128,
        fee: u128,
        decimal_diff: u8,
    ) -> (InitTransferMessage, FastFinTransferMsg) {
        let origin_amount = amount / 10u128.pow(decimal_diff.into());
        let origin_fee = fee / 10u128.pow(decimal_diff.into());

        let transfer_msg = InitTransferMessage {
            origin_nonce: 0,
            token: env.eth_token_address.clone(),
            recipient: OmniAddress::Near(account_n(1)),
            amount: U128(origin_amount + origin_fee),
            fee: Fee {
                fee: U128(origin_fee),
                native_fee: U128(0),
            },
            sender: eth_eoa_address(),
            msg: String::default(),
            emitter_address: eth_factory_address(),
        };

        let fast_transfer_msg = get_fast_transfer_msg(env, transfer_msg.clone());

        (transfer_msg, fast_transfer_msg)
    }

    fn get_transfer_to_other_chain_msg(
        env: &TestEnv,
        amount: u128,
        fee: u128,
        decimal_diff: u8,
    ) -> (InitTransferMessage, FastFinTransferMsg) {
        let origin_amount = amount / 10u128.pow(decimal_diff.into());
        let origin_fee = fee / 10u128.pow(decimal_diff.into());

        let transfer_msg = InitTransferMessage {
            origin_nonce: 0,
            token: env.eth_token_address.clone(),
            recipient: base_eoa_address(),
            amount: U128(origin_amount + origin_fee),
            fee: Fee {
                fee: U128(origin_fee),
                native_fee: U128(0),
            },
            sender: eth_eoa_address(),
            msg: String::default(),
            emitter_address: eth_factory_address(),
        };

        let fast_transfer_msg = get_fast_transfer_msg(env, transfer_msg.clone());

        (transfer_msg, fast_transfer_msg)
    }

    fn get_fast_transfer_msg(
        env: &TestEnv,
        transfer_msg: InitTransferMessage,
    ) -> FastFinTransferMsg {
        FastFinTransferMsg {
            transfer_id: TransferId {
                origin_chain: transfer_msg.sender.get_chain(),
                origin_nonce: transfer_msg.origin_nonce,
            },
            recipient: transfer_msg.recipient.clone(),
            fee: transfer_msg.fee,
            msg: transfer_msg.msg,
            amount: transfer_msg.amount,
            storage_deposit_amount: match transfer_msg.recipient.get_chain() {
                ChainKind::Near => Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                _ => None,
            },
            relayer: env.relayer_account.id().clone(),
        }
    }
}
