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
        BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, Fee, OmniAddress, TransferId,
        TransferIdKind, TransferMessage, UnifiedTransferId,
    };
    use rstest::rstest;

    use crate::helpers::tests::{
        account_n, base_eoa_address, build_artifacts, eth_eoa_address, eth_factory_address,
        fast_relayer_account_id, relayer_account_id, NEP141_DEPOSIT,
    };
    use crate::{environment::*, helpers::tests::BuildArtifacts};

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
        async fn new(
            build_artifacts: &BuildArtifacts,
            is_bridged_token: bool,
        ) -> anyhow::Result<Self> {
            let sender_balance_token = 1_000_000_000_000;

            let env_builder = if is_bridged_token {
                TestEnvBuilder::new(build_artifacts.clone())
                    .await?
                    .with_bridged_eth()
                    .await?
            } else {
                TestEnvBuilder::new(build_artifacts.clone())
                    .await?
                    .with_native_nep141_token(18)
                    .await?
            };

            let relayer_account = env_builder.create_account(relayer_account_id()).await?;
            let fast_relayer_account = env_builder
                .create_account(fast_relayer_account_id())
                .await?;
            let _ = env_builder.create_account(account_n(1)).await?;

            env_builder.storage_deposit(relayer_account.id()).await?;
            env_builder
                .storage_deposit(fast_relayer_account.id())
                .await?;

            env_builder
                .mint_tokens(relayer_account.id(), sender_balance_token)
                .await?;
            env_builder
                .mint_tokens(fast_relayer_account.id(), sender_balance_token * 2)
                .await?;
            env_builder
                .mint_tokens(
                    &env_builder.bridge_contract.id().clone(),
                    sender_balance_token / 2,
                )
                .await?;

            Ok(Self {
                token_contract: env_builder.token.contract,
                eth_token_address: env_builder.token.eth_address,
                bridge_contract: env_builder.bridge_contract,
                relayer_account,
                fast_relayer_account,
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
            .args_json(json!({
                "msg": None::<String>,
            }))
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
            .args_json(json!({
                "msg": None::<String>,
            }))
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

    fn default_fast_transfer_native() -> FastTransferParams {
        FastTransferParams {
            amount_to_send: 100_000_000,
            fast_transfer_msg: FastFinTransferMsg {
                transfer_id: UnifiedTransferId {
                    origin_chain: ChainKind::Eth,
                    kind: TransferIdKind::Nonce(0),
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
                transfer_id: UnifiedTransferId {
                    origin_chain: ChainKind::Eth,
                    kind: TransferIdKind::Nonce(0),
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
            transfer_id: UnifiedTransferId {
                origin_chain: transfer_msg.sender.get_chain(),
                kind: TransferIdKind::Nonce(transfer_msg.origin_nonce),
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
            is_bridged_token: bool,
            error: Option<&str>,
        ) -> anyhow::Result<()> {
            let OmniAddress::Near(recipient) = params.fast_transfer_msg.recipient.clone() else {
                panic!("Recipient is not a Near address");
            };
            let origin_chain = params.fast_transfer_msg.transfer_id.origin_chain;

            let locked_before =
                get_locked_tokens(&env.bridge_contract, origin_chain, env.token_contract.id())
                    .await?;
            let recipient_balance_before = get_balance(&env.token_contract, &recipient).await?;
            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            let result =
                do_fast_transfer(env, params.amount_to_send, params.fast_transfer_msg, None)
                    .await?;

            let recipient_balance_after = get_balance(&env.token_contract, &recipient).await?;
            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;
            let locked_after =
                get_locked_tokens(&env.bridge_contract, origin_chain, env.token_contract.id())
                    .await?;

            if let Some(error_msg) = error {
                assert!(
                    has_error_message(&result, error_msg),
                    "Expected error message: {error_msg}"
                );

                assert_eq!(recipient_balance_before, recipient_balance_after);
                assert_eq!(contract_balance_before, contract_balance_after);
                assert_eq!(relayer_balance_before, relayer_balance_after);
                assert_eq!(locked_before, locked_after);

                return Ok(());
            }

            assert_eq!(0, result.failures().len());

            if is_bridged_token {
                assert_eq!(0, locked_before.0);
                assert_eq!(0, locked_after.0);
            } else {
                assert_eq!(locked_before.0, locked_after.0);
            }

            assert_eq!(
                params.amount_to_send,
                recipient_balance_after.0 - recipient_balance_before.0
            );
            assert_eq!(contract_balance_before, contract_balance_after);
            assert_eq!(
                relayer_balance_before,
                U128(relayer_balance_after.0 + params.amount_to_send)
            );
            assert_eq!(locked_before, locked_after);

            Ok(())
        }

        #[rstest]
        // Success case native token
        #[case(FastTransferCase {
            is_bridged_token: false,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
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
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee::default(),
                    amount: U128(100_000_000),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                }
            },
            error: None,
        })]
        // Success case bridged token with fee
        #[case(FastTransferCase {
            is_bridged_token: true,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee {
                        fee: U128(10_000),
                        native_fee: U128(0),
                    },
                    amount: U128(100_010_000),
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
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
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
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
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
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee::default(),
                    amount: U128(100_000_000),
                    msg: String::default(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.saturating_mul(100).as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                },
            },
            error: Some("Not enough storage deposited"),
        })]
        // Refund on ft_transfer_call failure
        #[case(FastTransferCase {
            is_bridged_token: true,
            transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
                    },
                    recipient: OmniAddress::Near(account_n(1)),
                    fee: Fee::default(),
                    amount: U128(100_000_000),
                    msg: "Receiver can't accept ft_transfer_call".to_string(),
                    storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
                    relayer: AccountId::from_str("fake.testnet").unwrap(),
                },
            },
            error: Some("CodeDoesNotExist"),
        })]
        #[tokio::test]
        async fn single(
            build_artifacts: &BuildArtifacts,
            #[case] mut case: FastTransferCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts, case.is_bridged_token).await?;
            case.transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            assert_transfer_to_near(&env, case.transfer, case.is_bridged_token, case.error).await
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
            build_artifacts: &BuildArtifacts,
            #[case] mut case: FastTransferMultipleCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts, case.is_bridged_token).await?;
            case.first_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();
            case.second_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            assert_transfer_to_near(&env, case.first_transfer, case.is_bridged_token, None).await?;
            assert_transfer_to_near(
                &env,
                case.second_transfer,
                case.is_bridged_token,
                case.error,
            )
            .await
        }
    }

    mod transfer_to_other_chain {
        use crate::helpers::tests::build_artifacts;

        use super::*;

        #[allow(clippy::too_many_lines)]
        async fn assert_transfer_to_other_chain(
            env: &TestEnv,
            params: FastTransferParams,
            is_bridged_token: bool,
            error: Option<&str>,
        ) -> anyhow::Result<()> {
            let token_decimal_diff = params.amount_to_send
                / (params.fast_transfer_msg.amount.0 - params.fast_transfer_msg.fee.fee.0);
            let normalized_fee = params.fast_transfer_msg.fee.fee.0 * token_decimal_diff;
            let origin_chain = params.fast_transfer_msg.transfer_id.origin_chain;
            let destination_chain = params.fast_transfer_msg.recipient.get_chain();

            let locked_origin_before =
                get_locked_tokens(&env.bridge_contract, origin_chain, env.token_contract.id())
                    .await?;
            let locked_destination_before = get_locked_tokens(
                &env.bridge_contract,
                destination_chain,
                env.token_contract.id(),
            )
            .await?;
            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_before =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            let result = do_fast_transfer(
                env,
                params.amount_to_send,
                params.fast_transfer_msg.clone(),
                None,
            )
            .await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;
            let locked_origin_after =
                get_locked_tokens(&env.bridge_contract, origin_chain, env.token_contract.id())
                    .await?;
            let locked_destination_after = get_locked_tokens(
                &env.bridge_contract,
                destination_chain,
                env.token_contract.id(),
            )
            .await?;

            if let Some(error_msg) = error {
                assert!(
                    has_error_message(&result, error_msg),
                    "Expected error message: {error_msg}"
                );

                assert!(relayer_balance_after == relayer_balance_before);
                assert!(contract_balance_after == contract_balance_before);
                assert_eq!(locked_origin_before, locked_origin_after);
                assert_eq!(locked_destination_before, locked_destination_after);

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

            assert_eq!(locked_origin_before, locked_origin_after);

            if is_bridged_token {
                assert_eq!(
                    locked_destination_after,
                    U128(locked_destination_before.0 + transfer_message.amount.0)
                );
                assert_eq!(contract_balance_before, contract_balance_after);
            } else {
                assert_eq!(
                    locked_destination_after,
                    U128(locked_destination_before.0 + transfer_message.amount.0)
                );
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
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
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
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
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
            build_artifacts: &BuildArtifacts,
            #[case] mut case: FastTransferCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts, case.is_bridged_token).await?;
            case.transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            assert_transfer_to_other_chain(&env, case.transfer, case.is_bridged_token, case.error)
                .await
        }

        #[rstest]
        // Fails due to duplicate transfer
        #[case(FastTransferMultipleCase {
            is_bridged_token: false,
            first_transfer: FastTransferParams {
                amount_to_send: 100_000_000,
                fast_transfer_msg: FastFinTransferMsg {
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
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
                    transfer_id: UnifiedTransferId {
                        origin_chain: ChainKind::Eth,
                        kind: TransferIdKind::Nonce(0),
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
            build_artifacts: &BuildArtifacts,
            #[case] mut case: FastTransferMultipleCase,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts, case.is_bridged_token).await?;
            case.first_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();
            case.second_transfer.fast_transfer_msg.relayer = env.relayer_account.id().clone();

            assert_transfer_to_other_chain(&env, case.first_transfer, case.is_bridged_token, None)
                .await?;

            assert_transfer_to_other_chain(
                &env,
                case.second_transfer,
                case.is_bridged_token,
                case.error,
            )
            .await
        }

        #[rstest]
        #[tokio::test]
        async fn fails_due_to_already_finalised(
            build_artifacts: &BuildArtifacts,
        ) -> anyhow::Result<()> {
            let env = TestEnv::new(build_artifacts, true).await?;

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

            let result = do_fast_transfer(&env, transfer_amount, fast_transfer_msg, None).await?;
            assert!(has_error_message(&result, "ERR_TRANSFER_ALREADY_FINALISED"));

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let contract_balance_after =
                get_balance(&env.token_contract, env.bridge_contract.id()).await?;

            assert_eq!(relayer_balance_before, relayer_balance_after);
            assert_eq!(contract_balance_before, contract_balance_after);

            Ok(())
        }
    }

    mod finalisation {
        use super::*;

        struct FinalisationParams<'a> {
            fast_transfer_amount: u128,
            transfer_msg: InitTransferMessage,
            fast_relayer_account: Option<&'a near_workspaces::Account>,
        }

        async fn assert_finalisation(
            env: &TestEnv,
            params: FinalisationParams<'_>,
            error: Option<&str>,
        ) -> anyhow::Result<()> {
            let token_decimal_diff = params.fast_transfer_amount
                / (params.transfer_msg.amount.0 - params.transfer_msg.fee.fee.0);

            // If destination is Near, we expect the fee to be paid to the relayer
            let expected_to_receive = if let OmniAddress::Near(_) =
                params.transfer_msg.recipient.clone()
            {
                params.transfer_msg.amount.0 * token_decimal_diff
            } else {
                (params.transfer_msg.amount.0 - params.transfer_msg.fee.fee.0) * token_decimal_diff
            };

            let relayer_balance_before =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let fast_relayer_balance_before =
                get_balance(&env.token_contract, env.fast_relayer_account.id()).await?;

            let result =
                do_fin_transfer(env, params.transfer_msg, params.fast_relayer_account).await?;

            let relayer_balance_after =
                get_balance(&env.token_contract, env.relayer_account.id()).await?;
            let fast_relayer_balance_after =
                get_balance(&env.token_contract, env.fast_relayer_account.id()).await?;

            if let Some(error_msg) = error {
                assert!(
                    has_error_message(&result, error_msg),
                    "Expected error message: {error_msg}"
                );

                assert!(relayer_balance_after.0 == relayer_balance_before.0);
                assert!(fast_relayer_balance_after.0 == fast_relayer_balance_before.0);

                return Ok(());
            }

            assert_eq!(0, result.failures().len());

            if params.fast_relayer_account.is_some() {
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

            Ok(())
        }

        mod to_near {
            use super::*;

            #[rstest]
            #[tokio::test]
            async fn succeeds(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
                let env = TestEnv::new(build_artifacts, false).await?;

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
                    transfer_id: UnifiedTransferId {
                        origin_chain: transfer_msg.sender.get_chain(),
                        kind: TransferIdKind::Nonce(transfer_msg.origin_nonce),
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

                assert_finalisation(
                    &env,
                    FinalisationParams {
                        fast_transfer_amount,
                        transfer_msg,
                        fast_relayer_account: Some(&env.fast_relayer_account),
                    },
                    None,
                )
                .await
            }

            #[rstest]
            #[tokio::test]
            async fn fails_due_to_duplicate_finalisation(
                build_artifacts: &BuildArtifacts,
            ) -> anyhow::Result<()> {
                let env = TestEnv::new(build_artifacts, false).await?;

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
                    transfer_id: UnifiedTransferId {
                        origin_chain: transfer_msg.sender.get_chain(),
                        kind: TransferIdKind::Nonce(transfer_msg.origin_nonce),
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

                let _ =
                    do_fast_transfer(&env, fast_transfer_amount, fast_transfer_msg.clone(), None)
                        .await?;

                assert_finalisation(
                    &env,
                    FinalisationParams {
                        fast_transfer_amount,
                        transfer_msg: transfer_msg.clone(),
                        fast_relayer_account: None,
                    },
                    None,
                )
                .await?;

                assert_finalisation(
                    &env,
                    FinalisationParams {
                        fast_transfer_amount,
                        transfer_msg: transfer_msg.clone(),
                        fast_relayer_account: None,
                    },
                    Some("The transfer is already finalised"),
                )
                .await
            }
        }

        mod to_other_chain {
            use super::*;

            #[rstest]
            #[tokio::test]
            async fn succeeds(build_artifacts: &BuildArtifacts) -> anyhow::Result<()> {
                let env = TestEnv::new(build_artifacts, false).await?;

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
                    do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None)
                        .await?;
                assert_eq!(0, result.failures().len());

                assert_finalisation(
                    &env,
                    FinalisationParams {
                        fast_transfer_amount: transfer_amount,
                        transfer_msg,
                        fast_relayer_account: None,
                    },
                    None,
                )
                .await
            }

            #[rstest]
            #[tokio::test]
            async fn fails_due_to_duplicate_finalisation(
                build_artifacts: &BuildArtifacts,
            ) -> anyhow::Result<()> {
                let env = TestEnv::new(build_artifacts, false).await?;

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
                    do_fast_transfer(&env, transfer_amount, fast_transfer_msg.clone(), None)
                        .await?;
                assert_eq!(0, result.failures().len());

                assert_finalisation(
                    &env,
                    FinalisationParams {
                        fast_transfer_amount: transfer_amount,
                        transfer_msg: transfer_msg.clone(),
                        fast_relayer_account: None,
                    },
                    None,
                )
                .await?;

                assert_finalisation(
                    &env,
                    FinalisationParams {
                        fast_transfer_amount: transfer_amount,
                        transfer_msg,
                        fast_relayer_account: Some(&env.fast_relayer_account),
                    },
                    Some("The transfer is already finalised"),
                )
                .await
            }
        }
    }
}
