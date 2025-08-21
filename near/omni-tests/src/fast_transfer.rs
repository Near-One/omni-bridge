#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use near_sdk::{
        borsh,
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{result::ExecutionFinalResult, types::NearToken};
    use omni_types::{
        locker_args::{FinTransferArgs, StorageDepositAction},
        prover_result::{InitTransferMessage, ProverResult},
        BasicMetadata, BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, Fee, OmniAddress,
        TransferId, TransferMessage,
    };
    use rstest::rstest;

    use crate::helpers::tests::{
        account_n, base_eoa_address, base_factory_address, eth_eoa_address, eth_factory_address,
        eth_token_address, fast_relayer_account_id, get_bind_token_args, locker_wasm,
        mock_prover_wasm, mock_token_wasm, relayer_account_id, token_deployer_wasm, NEP141_DEPOSIT,
    };

    struct FastTransferParams {
        amount_to_send: u128,
        fast_transfer_msg: FastFinTransferMsg,
    }

    struct FastTransferCase {
        is_bridged_token: bool,
        transfer: FastTransferParams,
        error: Option<&'static str>,
    }

    struct FastTransferMultipleCase {
        is_bridged_token: bool,
        first_transfer: FastTransferParams,
        second_transfer: FastTransferParams,
        error: Option<&'static str>,
    }

    struct TestEnv {
        token_contract: near_workspaces::Contract,
        eth_token_address: OmniAddress,
        bridge_contract: near_workspaces::Contract,
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

            let prover_contract = worker.dev_deploy(&mock_prover_wasm()).await?;
            // Deploy and initialize bridge
            let bridge_contract = worker.dev_deploy(&locker_wasm()).await?;
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
                    "controller": bridge_contract.id(),
                    "dao": AccountId::from_str("dao.near").unwrap(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            bridge_contract
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
                    Self::deploy_bridged_token(&worker, &bridge_contract).await?;

                // Mint to relayer account
                Self::fake_finalize_transfer(
                    &bridge_contract,
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
                    &bridge_contract,
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
                        "account_id": bridge_contract.id(),
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
                    Self::deploy_native_token(worker, &bridge_contract, eth_factory_address)
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
                        "account_id": bridge_contract.id(),
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
                        "receiver_id": bridge_contract.id(),
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
                    "receiver_id": bridge_contract.id(),
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
                bridge_contract,
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
                    chain_kind: ChainKind::Near,
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
    ) -> anyhow::Result<ExecutionFinalResult> {
        let relayer_account = relayer_account.unwrap_or(&env.relayer_account);

        let storage_deposit_amount = match fast_transfer_msg.recipient {
            OmniAddress::Near(_) => {
                get_balance_required_for_fast_transfer_to_near(&env.bridge_contract, true).await?
            }
            _ => {
                get_balance_required_for_fast_transfer_to_other_chain(&env.bridge_contract).await?
            }
        };

        // Deposit to the storage
        relayer_account
            .call(env.bridge_contract.id(), "storage_deposit")
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
                "receiver_id": env.bridge_contract.id(),
                "amount": U128(transfer_amount),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::FastFinTransfer(fast_transfer_msg))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?;

        Ok(transfer_result)
    }

    async fn do_fin_transfer(
        env: &TestEnv,
        transfer_msg: InitTransferMessage,
        fast_relayer_account: Option<&near_workspaces::Account>,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let fast_relayer_account = fast_relayer_account.unwrap_or(&env.relayer_account);

        let required_balance_for_fin_transfer: NearToken = env
            .bridge_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;

        let required_balance_for_init_transfer: NearToken = env
            .bridge_contract
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
            .call(env.bridge_contract.id(), "fin_transfer")
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
            .await?;

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

    fn has_error_message(result: ExecutionFinalResult, error_msg: &str) -> bool {
        result.failures().into_iter().any(|outcome| {
            outcome
                .clone()
                .into_result()
                .is_err_and(|err| format!("{err:?}").contains(error_msg))
        })
    }

    fn default_fast_transfer_native() -> FastTransferParams {
        FastTransferParams {
            amount_to_send: 100_000_000,
            fast_transfer_msg: FastFinTransferMsg {
                transfer_id: TransferId {
                    origin_chain: ChainKind::Eth,
                    origin_nonce: 0,
                },
                recipient: OmniAddress::Near(account_n(1)),
                fee: Fee {
                    fee: U128(1),
                    native_fee: U128(0),
                },
                amount: U128(101),
                msg: String::default(),
                storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                relayer: AccountId::from_str("fake.testnet").unwrap(),
            },
        }
    }

    fn default_fast_transfer_bridged() -> FastTransferParams {
        FastTransferParams {
            amount_to_send: 100_000_000,
            fast_transfer_msg: FastFinTransferMsg {
                transfer_id: TransferId {
                    origin_chain: ChainKind::Eth,
                    origin_nonce: 0,
                },
                recipient: OmniAddress::Near(account_n(1)),
                fee: Fee {
                    fee: U128(0),
                    native_fee: U128(0),
                },
                amount: U128(100_000_000),
                msg: String::default(),
                storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                relayer: AccountId::from_str("fake.testnet").unwrap(),
            },
        }
    }

    fn fast_transfer_native(f: impl FnOnce(&mut FastTransferParams)) -> FastTransferParams {
        let mut params = default_fast_transfer_native();
        f(&mut params);
        params
    }

    fn get_fast_transfer_msg_from_init_transfer(
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

    mod transfer_to_near {
        use super::*;

        async fn assert_transfer_to_near(
            env: &TestEnv,
            params: FastTransferParams,
            error: Option<&str>,
        ) -> anyhow::Result<()> {
            let OmniAddress::Near(recipient) = params.fast_transfer_msg.recipient.clone() else {
                panic!("Recipient is not a Near address");
            };

            let recipient_balance_before = get_balance(&env.token_contract, &recipient).await?;
            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            let result =
                do_fast_transfer(&env, params.amount_to_send, params.fast_transfer_msg, None)
                    .await?;

            let recipient_balance_after = get_balance(&env.token_contract, &recipient).await?;
            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            if let Some(error_msg) = error {
                assert!(
                    has_error_message(result, error_msg),
                    "Expected error message: {error_msg}"
                );

                assert_eq!(recipient_balance_before, recipient_balance_after);
                assert_eq!(contract_balance_before, contract_balance_after);
                assert_eq!(relayer_balance_before, relayer_balance_after);

                return Ok(());
            }

            assert_eq!(0, result.failures().len());

            assert_eq!(
                params.amount_to_send,
                recipient_balance_after.0 - recipient_balance_before.0
            );
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + params.amount_to_send)
            );

            Ok(())
        }

        #[rstest]
        // Success case native token
        #[case(FastTransferCase {
            is_bridged_token: false,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee {
                        fee: U128(1),
                        native_fee: U128(0),
                    },
                    amount: U128(101),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            error: None,
        })]
        // Success case bridged token
        #[case(FastTransferCase {
            is_bridged_token: true,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee {
                        fee: U128(0),
                        native_fee: U128(0),
                    },
                    amount: U128(100_000_000),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            error: None,
        })]
        // Amount in FastFinTransferMsg doesn't include fee
        #[case(FastTransferCase {
            is_bridged_token: false,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee {
                        fee: U128(1),
                        native_fee: U128(0),
                    },
                    amount: U128(100),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                },
            },
            error: Some("ERR_INVALID_FAST_TRANSFER_AMOUNT"),
        })]
        // Invalid fee passed in FastFinTransferMsg
        #[case(FastTransferCase {
            is_bridged_token: false,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee {
                        fee: U128(2),
                        native_fee: U128(0),
                    },
                    amount: U128(101),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            error: Some("ERR_INVALID_FAST_TRANSFER_AMOUNT"),
        })]
        // Invalid storage deposit amount
        #[case(FastTransferCase {
            is_bridged_token: true,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee {
                        fee: U128(0),
                        native_fee: U128(0),
                    },
                    amount: U128(100_000_000),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.saturating_mul(100).as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                },
            },
            error: Some("Not enough storage deposited"),
        })]
        #[tokio::test]
        async fn single(
            #[case] mut case: FastTransferCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(case.is_bridged_token).await?;
            case.transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            assert_transfer_to_near(&env, case.transfer, case.error).await
        }

        #[rstest]
        // Success with two different transfers
        #[case(FastTransferMultipleCase {
            is_bridged_token: false,
            first_transfer: default_fast_transfer_native(),
            second_transfer: fast_transfer_native(|params| {
                params.amount_to_send = 104_000_000;
                params.fast_transfer_msg.amount = U128(104);
                params.fast_transfer_msg.fee.fee = U128(0);
            }),
            error: None,
        })]
        // Fails on duplicate fast transfer with native token
        #[case(FastTransferMultipleCase {
            is_bridged_token: false,
            first_transfer: default_fast_transfer_native(),
            second_transfer: default_fast_transfer_native(),
            error: Some("Fast transfer is already performed"),
        })]
        // Fails on duplicate fast transfer with bridged token
        #[case(FastTransferMultipleCase {
            is_bridged_token: true,
            first_transfer: default_fast_transfer_bridged(),
            second_transfer: default_fast_transfer_bridged(),
            error: Some("Fast transfer is already performed"),
        })]
        #[tokio::test]
        async fn multiple(
            #[case] mut case: FastTransferMultipleCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(case.is_bridged_token).await?;
            case.first_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();
            case.second_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            let _ = assert_transfer_to_near(&env, case.first_transfer, None).await?;
            assert_transfer_to_near(&env, case.second_transfer, case.error).await
        }
    }

    mod finalisation_to_near {
        use super::*;

        struct FinalisationToNearParams<'a> {
            fast_transfer_amount: u128,
            transfer_msg: InitTransferMessage,
            fast_relayer_account: Option<&'a near_workspaces::Account>,
        }

        async fn assert_finalisation_to_near(
            env: &TestEnv,
            params: FinalisationToNearParams<'_>,
            error: Option<&str>,
        ) -> anyhow::Result<()> {
            let OmniAddress::Near(recipient) = params.transfer_msg.recipient.clone() else {
                panic!("Recipient is not a Near address");
            };

            let token_decimal_diff = params.fast_transfer_amount
                / (params.transfer_msg.amount.0 - params.transfer_msg.fee.fee.0);
            let expected_to_receive = params.transfer_msg.amount.0 * token_decimal_diff;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let fast_relayer_balance_before =
                get_balance(&env.token_contract, env.fast_relayer_account.id()).await?;
            let recipient_balance_before = get_balance(&env.token_contract, &recipient).await?;

            let result =
                do_fin_transfer(&env, params.transfer_msg, params.fast_relayer_account).await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let fast_relayer_balance_after =
                get_balance(&env.token_contract, env.fast_relayer_account.id()).await?;
            let recipient_balance_after = get_balance(&env.token_contract, &recipient).await?;

            if let Some(error_msg) = error {
                assert!(
                    has_error_message(result, error_msg),
                    "Expected error message: {error_msg}"
                );

                assert!(relayer_balance_after.0 == relayer_balance_before.0);
                assert!(fast_relayer_balance_after.0 == fast_relayer_balance_before.0);
                assert!(recipient_balance_after.0 == recipient_balance_before.0);

                return Ok(());
            }

            if let Some(_) = params.fast_relayer_account {
                assert_eq!(
                    expected_to_receive,
                    fast_relayer_balance_after.0 - fast_relayer_balance_before.0
                );
                assert_eq!(relayer_balance_after, relayer_balance_before);
            } else {
                assert_eq!(
                    expected_to_receive,
                    relayer_balance_after.0 - relayer_balance_before.0
                );
                assert_eq!(fast_relayer_balance_after, fast_relayer_balance_before);
            }

            assert_eq!(recipient_balance_after, recipient_balance_before);

            Ok(())
        }

        #[tokio::test]
        async fn succeeds() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let fast_transfer_amount = 100_000_000;
            let transfer_msg = InitTransferMessage {
                origin_nonce: 0,
                token: env.eth_token_address.clone(),
                recipient: OmniAddress::Near(account_n(1)),
                amount: U128(101),
                fee: Fee {
                    fee: U128(1),
                    native_fee: U128(0),
                },
                sender: eth_eoa_address(),
                msg: String::default(),
                emitter_address: eth_factory_address(),
            };

            let fast_transfer_msg = FastFinTransferMsg {
                transfer_id: TransferId {
                    origin_chain: transfer_msg.sender.get_chain(),
                    origin_nonce: transfer_msg.origin_nonce,
                },
                recipient: transfer_msg.recipient.clone(),
                fee: transfer_msg.fee.clone(),
                msg: transfer_msg.msg.clone(),
                amount: transfer_msg.amount,
                storage_deposit_amount: match transfer_msg.recipient.get_chain() {
                    ChainKind::Near => Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    _ => None,
                },
                relayer: env.fast_relayer_account.id().clone(),
            };

            let _ = do_fast_transfer(
                &env,
                fast_transfer_amount,
                fast_transfer_msg.clone(),
                Some(&env.fast_relayer_account),
            )
            .await?;

            assert_finalisation_to_near(
                &env,
                FinalisationToNearParams {
                    fast_transfer_amount,
                    transfer_msg,
                    fast_relayer_account: Some(&env.fast_relayer_account),
                },
                None,
            )
            .await
        }

        #[tokio::test]
        async fn fails_due_to_duplicate_finalisation() -> anyhow::Result<()> {
            let env = TestEnv::new_with_native_token().await?;

            let fast_transfer_amount = 100_000_000;
            let transfer_msg = InitTransferMessage {
                origin_nonce: 0,
                token: env.eth_token_address.clone(),
                recipient: OmniAddress::Near(account_n(1)),
                amount: U128(101),
                fee: Fee {
                    fee: U128(1),
                    native_fee: U128(0),
                },
                sender: eth_eoa_address(),
                msg: String::default(),
                emitter_address: eth_factory_address(),
            };

            let fast_transfer_msg = FastFinTransferMsg {
                transfer_id: TransferId {
                    origin_chain: transfer_msg.sender.get_chain(),
                    origin_nonce: transfer_msg.origin_nonce,
                },
                recipient: transfer_msg.recipient.clone(),
                fee: transfer_msg.fee.clone(),
                msg: transfer_msg.msg.clone(),
                amount: transfer_msg.amount,
                storage_deposit_amount: match transfer_msg.recipient.get_chain() {
                    ChainKind::Near => Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    _ => None,
                },
                relayer: env.relayer_account.id().clone(),
            };

            let _ = do_fast_transfer(&env, fast_transfer_amount, fast_transfer_msg.clone(), None)
                .await?;

            assert_finalisation_to_near(
                &env,
                FinalisationToNearParams {
                    fast_transfer_amount,
                    transfer_msg: transfer_msg.clone(),
                    fast_relayer_account: None,
                },
                None,
            )
            .await?;

            assert_finalisation_to_near(
                &env,
                FinalisationToNearParams {
                    fast_transfer_amount,
                    transfer_msg: transfer_msg.clone(),
                    fast_relayer_account: None,
                },
                Some("The transfer is already finalised"),
            )
            .await
        }
    }

    mod transfer_to_other_chain {
        use super::*;

        async fn assert_transfer_to_other_chain(
            env: &TestEnv,
            params: FastTransferParams,
            is_bridged_token: bool,
            error: Option<&str>,
        ) -> anyhow::Result<()> {
            let token_decimal_diff = params.amount_to_send
                / (params.fast_transfer_msg.amount.0 - params.fast_transfer_msg.fee.fee.0);
            let normalized_fee = params.fast_transfer_msg.fee.fee.0 * token_decimal_diff;

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            let result = do_fast_transfer(
                &env,
                params.amount_to_send,
                params.fast_transfer_msg.clone(),
                None,
            )
            .await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            if let Some(error_msg) = error {
                assert!(
                    has_error_message(result, error_msg),
                    "Expected error message: {error_msg}"
                );

                assert!(relayer_balance_after == relayer_balance_before);
                assert!(contract_balance_after == contract_balance_before);

                return Ok(());
            }

            assert_eq!(0, result.failures().len());

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
            assert_eq!(
                params.amount_to_send + normalized_fee,
                transfer_message.amount.0
            );
            assert_eq!(
                params.fast_transfer_msg.recipient,
                transfer_message.recipient
            );
            assert_eq!(
                params.fast_transfer_msg.fee.native_fee,
                transfer_message.fee.native_fee
            );
            assert_eq!(normalized_fee, transfer_message.fee.fee.0);
            assert_eq!(params.fast_transfer_msg.msg, transfer_message.msg);
            assert_eq!(
                OmniAddress::Near(env.relayer_account.id().clone()),
                transfer_message.sender
            );

            if is_bridged_token {
                assert_eq!(contract_balance_after, contract_balance_before);
            } else {
                assert_eq!(
                    contract_balance_before,
                    U128(contract_balance_after.0 - params.amount_to_send)
                );
            }

            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + params.amount_to_send)
            );

            Ok(())
        }

        #[rstest]
        // Success case for native token
        #[case(FastTransferCase {
            is_bridged_token: false,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: base_eoa_address(),
                    fee: Fee {
                        fee: U128(1),
                        native_fee: U128(0),
                    },
                    amount: U128(101),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            error: None,
        })]
        // Success case for bridged token
        #[case(FastTransferCase {
            is_bridged_token: true,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: base_eoa_address(),
                    fee: Fee {
                        fee: U128(1_000_000),
                        native_fee: U128(0),
                    },
                    amount: U128(101_000_000),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            error: None,
        })]
        #[tokio::test]
        async fn test_transfer_to_other_chain(
            #[case] mut case: FastTransferCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(case.is_bridged_token).await?;
            case.transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            assert_transfer_to_other_chain(
                &env,
                case.transfer,
                case.is_bridged_token,
                case.error,
            )
            .await
        }

        #[rstest]
        // Fails due to duplicate transfer
        #[case(FastTransferMultipleCase {
            is_bridged_token: false,
            first_transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: base_eoa_address(),
                    fee: Fee {
                        fee: U128(1),
                        native_fee: U128(0),
                    },
                    amount: U128(101),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            second_transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: TransferId {
                        origin_chain: ChainKind::Eth,
                        origin_nonce: 0,
                    },
                    recipient: base_eoa_address(),
                    fee: Fee {
                        fee: U128(1),
                        native_fee: U128(0),
                    },
                    amount: U128(101),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            error: Some("Fast transfer is already performed"),
        })]
        #[tokio::test]
        async fn test_transfer_to_other_chain_multiple(
            #[case] mut case: FastTransferMultipleCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(case.is_bridged_token).await?;
            case.first_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();
            case.second_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            let _ = assert_transfer_to_other_chain(
                &env,
                case.first_transfer,
                case.is_bridged_token,
                None,
            )
            .await?;

            assert_transfer_to_other_chain(
                &env,
                case.second_transfer,
                case.is_bridged_token,
                case.error,
            )
            .await
        }

        #[tokio::test]
        async fn fails_due_to_already_finalised() -> anyhow::Result<()> {
            let env = TestEnv::new_with_bridged_token().await?;

            let transfer_amount = 100_000_000;
            let transfer_msg = InitTransferMessage {
                origin_nonce: 0,
                token: env.eth_token_address.clone(),
                recipient: base_eoa_address(),
                amount: U128(101_000_000),
                fee: Fee {
                    fee: U128(1_000_000),
                    native_fee: U128(0),
                },
                sender: eth_eoa_address(),
                msg: String::default(),
                emitter_address: eth_factory_address(),
            };
            let fast_transfer_msg =
                get_fast_transfer_msg_from_init_transfer(&env, transfer_msg.clone());

            let result = do_fin_transfer(&env, transfer_msg, None).await?;
            assert_eq!(0, result.failures().len());

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            assert_eq!(U128(transfer_amount), contract_balance_before);

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

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
            let transfer_msg = InitTransferMessage {
                origin_nonce: 0,
                token: env.eth_token_address.clone(),
                recipient: base_eoa_address(),
                amount: U128(101),
                fee: Fee {
                    fee: U128(1),
                    native_fee: U128(0),
                },
                sender: eth_eoa_address(),
                msg: String::default(),
                emitter_address: eth_factory_address(),
            };
            let fast_transfer_msg =
                get_fast_transfer_msg_from_init_transfer(&env, transfer_msg.clone());

            let result =
                do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;
            assert_eq!(0, result.failures().len());

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;

            let result = do_fin_transfer(&env, transfer_msg, None).await?;
            assert_eq!(0, result.failures().len());

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
            let transfer_msg = InitTransferMessage {
                origin_nonce: 0,
                token: env.eth_token_address.clone(),
                recipient: base_eoa_address(),
                amount: U128(101),
                fee: Fee {
                    fee: U128(1),
                    native_fee: U128(0),
                },
                sender: eth_eoa_address(),
                msg: String::default(),
                emitter_address: eth_factory_address(),
            };
            let fast_transfer_msg =
                get_fast_transfer_msg_from_init_transfer(&env, transfer_msg.clone());

            let result =
                do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None).await?;
            assert_eq!(0, result.failures().len());

            let result = do_fin_transfer(&env, transfer_msg.clone(), None).await?;
            assert_eq!(0, result.failures().len());

            let result = do_fin_transfer(&env, transfer_msg, None).await?;

            assert!(has_error_message(
                result,
                "The transfer is already finalised"
            ));

            Ok(())
        }
    }
}
