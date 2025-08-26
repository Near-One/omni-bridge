#[cfg(test)]
mod tests {
    use near_sdk::{
        borsh,
        json_types::U128,
        near,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{network::Sandbox, types::NearToken, Contract, Worker};
    use omni_types::{
        locker_args::{BindTokenArgs, FinTransferArgs, StorageDepositAction},
        prover_result::{DeployTokenMessage, InitTransferMessage, ProverResult},
        BasicMetadata, Fee, OmniAddress,
    };
    use rand::RngCore;
    use rstest::rstest;

    use crate::helpers::tests::{
        account_n, eth_eoa_address, eth_factory_address, eth_token_address,
        get_test_deploy_token_args, locker_wasm, mock_token_receiver_wasm, mock_token_wasm,
        relayer_account_id, token_deployer_wasm, NEP141_DEPOSIT,
    };

    static HEX_STRING_2000: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        use std::fmt::Write;
        let mut bytes = [0u8; 2000];
        rand::rng().fill_bytes(&mut bytes);

        bytes.iter().fold(String::new(), |mut output, b| {
            let _ = write!(output, "{b:02X}");
            output
        })
    });

    #[near(serializers=[json])]
    struct TokenReceiverMessage {
        return_value: U128,
        panic: bool,
        extra_msg: String,
    }

    struct TestSetup {
        worker: Worker<Sandbox>,
        token_contract: Contract,
        locker_contract: Contract,
        token_receiver_contract: Contract,
        relayer_account: near_workspaces::Account,
        required_balance_for_fin_transfer: NearToken,
    }

    async fn setup_contracts(
        is_wnear: bool,
        deploy_minted_token: bool,
    ) -> anyhow::Result<TestSetup> {
        let worker = near_workspaces::sandbox().await?;

        // Deploy and init FT token
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

        let token_receiver_contract = worker.dev_deploy(&mock_token_receiver_wasm()).await?;

        // Deploy and init locker
        let locker_contract = worker.dev_deploy(&locker_wasm()).await?;
        let wnear_account_id: AccountId = if is_wnear {
            token_contract.id().clone()
        } else {
            "wnear.testnet".parse().unwrap()
        };
        locker_contract
            .call("new")
            .args_json(json!({
                "mpc_signer": "mpc.testnet",
                "nonce": U128(0),
                "wnear_account_id": wnear_account_id,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Get required balances
        let required_balance_for_fin_transfer: NearToken = locker_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;

        // Create relayer account
        let relayer_account = worker
            .create_tla(relayer_account_id(), worker.dev_generate().await.1)
            .await?
            .into_result()?;

        locker_contract
            .call("add_factory")
            .args_json(json!({
                "address": eth_factory_address(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let token_contract = if deploy_minted_token {
            setup_deployed_token(&worker, &locker_contract).await?
        } else {
            // Bind token
            let required_balance_for_bind_token: NearToken = locker_contract
                .view("required_balance_for_bind_token")
                .await?
                .json()?;

            relayer_account
                .call(locker_contract.id(), "bind_token")
                .args_borsh(BindTokenArgs {
                    chain_kind: omni_types::ChainKind::Eth,
                    prover_args: borsh::to_vec(&ProverResult::DeployToken(DeployTokenMessage {
                        token: token_contract.id().clone(),
                        token_address: eth_token_address(),
                        decimals: 24,
                        origin_decimals: 24,
                        emitter_address: eth_factory_address(),
                    }))
                    .unwrap(),
                })
                .deposit(required_balance_for_bind_token)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Storage deposit
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
        };

        Ok(TestSetup {
            worker,
            token_contract,
            locker_contract,
            token_receiver_contract,
            relayer_account,
            required_balance_for_fin_transfer,
        })
    }

    async fn setup_deployed_token(
        worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
        locker: &near_workspaces::Contract,
    ) -> anyhow::Result<near_workspaces::Contract> {
        // Setup token deployer
        let token_deployer = worker
            .create_tla_and_deploy(
                account_n(10),
                worker.dev_generate().await.1,
                &token_deployer_wasm(),
            )
            .await?
            .unwrap();

        token_deployer
            .call("new")
            .args_json(json!({
                "controller": locker.id(),
                "dao": "dao.near",
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let init_token_address = eth_token_address();
        // Configure locker
        locker
            .call("add_token_deployer")
            .args_json(json!({
                "chain": init_token_address.get_chain(),
                "account_id": token_deployer.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let token_deploy_initiator = worker
            .create_tla(account_n(2), worker.dev_generate().await.1)
            .await?
            .unwrap();

        let required_storage: NearToken = locker
            .view("required_balance_for_deploy_token")
            .await?
            .json()?;

        token_deploy_initiator
            .call(locker.id(), "deploy_token")
            .args_borsh(get_test_deploy_token_args(
                &init_token_address,
                &eth_factory_address(),
                &BasicMetadata {
                    name: "Test Token".to_string(),
                    symbol: "TEST".to_string(),
                    decimals: 18,
                },
            ))
            .deposit(required_storage)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let token_account_id: AccountId = locker
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

        Ok(token_contract)
    }

    #[rstest]
    #[case(vec![(account_n(1), true), (relayer_account_id(), true)], 1000, 1, None)]
    #[case(vec![(account_n(1), true)], 1000, 0, None)]
    #[case(
        vec![
            (account_n(1), true),
            (relayer_account_id(), true),
            (account_n(2), true),
            (account_n(2), true),
        ],
        1000,
        1,
        Some("Invalid len of accounts for storage deposit")
    )]
    #[case(
        vec![(relayer_account_id(), true), (account_n(1), true)],
        1000,
        1,
        Some("STORAGE_ERR: The transfer recipient is omitted")
    )]
    #[case(
        vec![(account_n(1), true)],
        1000,
        1,
        Some("STORAGE_ERR: The fee recipient is omitted")
    )]
    #[case(vec![], 1000, 1, Some("STORAGE_ERR: The transfer recipient is omitted"))]
    #[case(
        vec![(account_n(1), false), (relayer_account_id(), false)],
        1000,
        1,
        Some("STORAGE_ERR: The transfer recipient is omitted")
    )]
    #[case(
        vec![(account_n(1), true), (relayer_account_id(), false)],
        1000,
        1,
        Some("STORAGE_ERR: The fee recipient is omitted")
    )]
    #[case(
        vec![(account_n(1), false), (relayer_account_id(), true)],
        1000,
        1,
        Some("STORAGE_ERR: The transfer recipient is omitted")
    )]
    #[tokio::test]
    async fn test_storage_deposit_on_fin_transfer(
        #[case] storage_deposit_accounts: Vec<(AccountId, bool)>,
        #[case] amount: u128,
        #[case] fee: u128,
        #[case] expected_error: Option<&str>,
    ) {
        let result = internal_test_fin_transfer(
            storage_deposit_accounts,
            amount,
            fee,
            String::new(),
            amount - fee,
            fee,
            0,
            false, // is_deployed_token
        )
        .await;

        match result {
            Ok(()) => assert!(
                expected_error.is_none(),
                "Expected an error but got success"
            ),
            Err(result_error) => {
                let error = expected_error.unwrap_or_else(|| {
                    panic!("Got an error {result_error} when none was expected")
                });
                assert!(
                    result_error.to_string().contains(error),
                    "Wrong error. Got: {result_error}, Expected: {error}"
                );
            }
        }
    }

    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    async fn internal_test_fin_transfer(
        mut storage_deposit_accounts: Vec<(AccountId, bool)>,
        amount: u128,
        fee: u128,
        msg: String,
        expected_recipient_balance: u128,
        expected_relayer_balance: u128,
        expected_locker_balance: u128,
        is_deployed_token: bool,
    ) -> anyhow::Result<()> {
        let TestSetup {
            token_contract,
            locker_contract,
            relayer_account,
            token_receiver_contract,
            required_balance_for_fin_transfer,
            ..
        } = setup_contracts(false, is_deployed_token).await?;

        if !is_deployed_token {
            token_contract
                .call("ft_transfer")
                .args_json(json!({
                    "receiver_id": locker_contract.id(),
                    "amount": U128(amount),
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;
        }

        let recipient = if msg.is_empty() {
            account_n(1)
        } else {
            let receiver_contract = token_receiver_contract.id().clone();
            storage_deposit_accounts.insert(0, (receiver_contract.clone(), true));
            receiver_contract
        };

        let required_deposit_for_fin_transfer = NEP141_DEPOSIT
            .saturating_mul(u128::try_from(storage_deposit_accounts.len())?)
            .saturating_add(required_balance_for_fin_transfer);

        let storage_deposit_actions = storage_deposit_accounts
            .iter()
            .map(|(account_id, is_deposit_needed)| StorageDepositAction {
                token_id: token_contract.id().clone(),
                account_id: account_id.clone(),
                storage_deposit_amount: is_deposit_needed.then(|| NEP141_DEPOSIT.as_yoctonear()),
            })
            .collect();

        // Fin transfer
        relayer_account
            .call(locker_contract.id(), "fin_transfer")
            .args_borsh(FinTransferArgs {
                chain_kind: omni_types::ChainKind::Eth,
                storage_deposit_actions,
                prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                    origin_nonce: 1,
                    token: eth_token_address(),
                    recipient: OmniAddress::Near(recipient.clone()),
                    amount: U128(amount),
                    fee: Fee {
                        fee: U128(fee),
                        native_fee: U128(0),
                    },
                    sender: eth_eoa_address(),
                    msg,
                    emitter_address: eth_factory_address(),
                }))
                .unwrap(),
            })
            .deposit(required_deposit_for_fin_transfer)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Check balances
        let recipient_balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": recipient,
            }))
            .await?
            .json()?;
        assert_eq!(expected_recipient_balance, recipient_balance.0);

        let relayer_balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": relayer_account_id(),
            }))
            .await?
            .json()?;
        assert_eq!(expected_relayer_balance, relayer_balance.0);

        let locker_balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": locker_contract.id()
            }))
            .await?
            .json()?;
        assert_eq!(expected_locker_balance, locker_balance.0);

        Ok(())
    }

    #[rstest]
    #[case(50)]
    #[case(1_000_000)]
    #[tokio::test]
    async fn test_near_withdrawal(#[case] near_amount: u128) -> anyhow::Result<()> {
        let TestSetup {
            worker,
            token_contract,
            locker_contract,
            relayer_account,
            required_balance_for_fin_transfer,
            ..
        } = setup_contracts(true, false).await?;

        // Provide locker contract with large wNEAR balance
        let wnear_amount = NearToken::from_near(near_amount);

        // top up wNEAR contract with NEAR
        assert!(worker
            .root_account()?
            .transfer_near(token_contract.id(), wnear_amount)
            .await?
            .is_success());

        token_contract
            .call("ft_transfer")
            .args_json(json!({
                "receiver_id": locker_contract.id(),
                "amount": U128(wnear_amount.as_yoctonear()*2),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let recipient_account = worker
            .create_tla(account_n(1), worker.dev_generate().await.1)
            .await?
            .into_result()?;

        let storage_deposit_actions = vec![StorageDepositAction {
            token_id: token_contract.id().clone(),
            account_id: recipient_account.id().clone(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        }];

        let required_deposit_for_fin_transfer = NEP141_DEPOSIT
            .saturating_mul(u128::try_from(storage_deposit_actions.len())?)
            .saturating_add(required_balance_for_fin_transfer);

        // Try to finalize a large NEAR withdrawal
        let result = relayer_account
            .call(locker_contract.id(), "fin_transfer")
            .args_borsh(FinTransferArgs {
                chain_kind: omni_types::ChainKind::Eth,
                storage_deposit_actions,
                prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                    origin_nonce: 1,
                    token: eth_token_address(),
                    recipient: OmniAddress::Near(recipient_account.id().clone()),
                    amount: U128(wnear_amount.as_yoctonear()),
                    fee: Fee {
                        fee: U128(0),
                        native_fee: U128(0),
                    },
                    sender: eth_eoa_address(),
                    msg: String::default(),
                    emitter_address: eth_factory_address(),
                }))
                .unwrap(),
            })
            .deposit(required_deposit_for_fin_transfer)
            .max_gas()
            .transact()
            .await?;

        assert!(result.is_success(), "Fin transfer failed {result:?}");

        // Check that the NEAR balance of the recipient is greater or equal to 1000 NEAR
        let recipient_balance: NearToken =
            worker.view_account(recipient_account.id()).await?.balance;
        assert!(
            recipient_balance >= wnear_amount,
            "Recipient balance is {recipient_balance} while it should be at least {wnear_amount}"
        );

        Ok(())
    }

    struct FinTransferWithMsgCase {
        storage_deposit_accounts: Vec<(AccountId, bool)>,
        amount: u128,
        fee: u128,
        msg: TokenReceiverMessage,
        expected_recipient_balance: u128,
        expected_relayer_balance: u128,
        expected_locker_balance: u128,
    }

    #[rstest]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 999,
        expected_relayer_balance: 1,
        expected_locker_balance: 0,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![],
        amount: 1000,
        fee: 0,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 1000,
        expected_relayer_balance: 0,
        expected_locker_balance: 0,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(999),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 0,
        expected_relayer_balance: 0,
        expected_locker_balance: 1000,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(1),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 998,
        expected_relayer_balance: 1,
        expected_locker_balance: 1,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: true,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 0,
        expected_relayer_balance: 0,
        expected_locker_balance: 1000,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![],
        amount: 1000,
        fee: 0,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: true,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 0,
        expected_relayer_balance: 0,
        expected_locker_balance: 1000,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: false,
            extra_msg: HEX_STRING_2000.clone(),
        },
        expected_recipient_balance: 999,
        expected_relayer_balance: 1,
        expected_locker_balance: 0,
    })]
    #[tokio::test]
    async fn test_fin_transfer_with_msg(#[case] case: FinTransferWithMsgCase) {
        let msg = serde_json::to_string(&case.msg).unwrap();
        internal_test_fin_transfer(
            case.storage_deposit_accounts,
            case.amount,
            case.fee,
            msg,
            case.expected_recipient_balance,
            case.expected_relayer_balance,
            case.expected_locker_balance,
            false, // is_deployed_token
        )
        .await
        .unwrap();
    }

    #[rstest]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 999,
        expected_relayer_balance: 1,
        expected_locker_balance: 0,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![],
        amount: 1000,
        fee: 0,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 1000,
        expected_relayer_balance: 0,
        expected_locker_balance: 0,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(999),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 0,
        expected_relayer_balance: 0,
        expected_locker_balance: 0,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(1),
            panic: false,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 998,
        expected_relayer_balance: 1,
        expected_locker_balance: 1,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: true,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 0,
        expected_relayer_balance: 0,
        expected_locker_balance: 0,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![],
        amount: 1000,
        fee: 0,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: true,
            extra_msg: String::new(),
        },
        expected_recipient_balance: 0,
        expected_relayer_balance: 0,
        expected_locker_balance: 0,
    })]
    #[case(FinTransferWithMsgCase {
        storage_deposit_accounts: vec![(relayer_account_id(), true)],
        amount: 1000,
        fee: 1,
        msg: TokenReceiverMessage {
            return_value: U128(0),
            panic: false,
            extra_msg: HEX_STRING_2000.clone(),
        },
        expected_recipient_balance: 999,
        expected_relayer_balance: 1,
        expected_locker_balance: 0,
    })]
    #[tokio::test]
    async fn test_fin_transfer_with_msg_for_deployed_token(#[case] case: FinTransferWithMsgCase) {
        let msg = serde_json::to_string(&case.msg).unwrap();
        internal_test_fin_transfer(
            case.storage_deposit_accounts,
            case.amount,
            case.fee,
            msg,
            case.expected_recipient_balance,
            case.expected_relayer_balance,
            case.expected_locker_balance,
            true, // is_deployed_token
        )
        .await
        .unwrap();
    }
}
