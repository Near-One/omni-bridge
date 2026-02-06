#[cfg(test)]
mod tests {
    use near_api::{AccountId, NetworkConfig, Signer, Tokens};
    use near_sandbox::Sandbox;
    use near_sdk::{
        borsh,
        json_types::U128,
        near,
        serde_json::{self, json},
    };
    use near_token::NearToken;
    use omni_types::{
        locker_args::{FinTransferArgs, StorageDepositAction},
        prover_result::{InitTransferMessage, ProverResult},
        Fee, OmniAddress,
    };
    use rand::RngCore;
    use rstest::rstest;

    use crate::environment::{TestAccount, TestContract, TestEnvBuilder};
    use crate::helpers::tests::{
        account_n, build_artifacts, eth_eoa_address, eth_factory_address, eth_token_address,
        relayer_account_id, BuildArtifacts, NEP141_DEPOSIT,
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
        sandbox: Sandbox,
        network: NetworkConfig,
        token_contract: TestContract,
        locker_contract: TestContract,
        token_receiver_contract: TestContract,
        relayer_account: TestAccount,
        required_balance_for_fin_transfer: NearToken,
    }

    #[allow(clippy::too_many_lines)]
    async fn setup_contracts(
        is_wnear: bool,
        deploy_minted_token: bool,
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<TestSetup> {
        let env_builder = if is_wnear {
            TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .with_custom_wnear()
                .await?
        } else if deploy_minted_token {
            TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .with_bridged_token()
                .await?
        } else {
            TestEnvBuilder::new(build_artifacts.clone())
                .await?
                .with_native_nep141_token(24)
                .await?
        };

        let token_receiver_contract = env_builder.deploy_mock_receiver().await?;

        let relayer_account = env_builder.create_account(relayer_account_id()).await?;

        let required_balance_for_fin_transfer: NearToken = env_builder
            .bridge_contract
            .view_no_args("required_balance_for_fin_transfer", &env_builder.network)
            .await?;

        Ok(TestSetup {
            sandbox: env_builder.sandbox,
            network: env_builder.network,
            token_contract: env_builder.token.contract,
            locker_contract: env_builder.bridge_contract,
            token_receiver_contract,
            relayer_account,
            required_balance_for_fin_transfer,
        })
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
        Some("ERR_INVALID_STORAGE_ACCOUNTS_LEN")
    )]
    #[case(
        vec![(relayer_account_id(), true), (account_n(1), true)],
        1000,
        1,
        Some("ERR_STORAGE_RECIPIENT_OMITTED")
    )]
    #[case(
        vec![(account_n(1), true)],
        1000,
        1,
        Some("ERR_STORAGE_FEE_RECIPIENT_OMITTED")
    )]
    #[case(vec![], 1000, 1, Some("ERR_STORAGE_RECIPIENT_OMITTED"))]
    #[case(
        vec![(account_n(1), false), (relayer_account_id(), false)],
        1000,
        1,
        Some("ERR_STORAGE_RECIPIENT_OMITTED")
    )]
    #[case(
        vec![(account_n(1), true), (relayer_account_id(), false)],
        1000,
        1,
        Some("ERR_STORAGE_FEE_RECIPIENT_OMITTED")
    )]
    #[case(
        vec![(account_n(1), false), (relayer_account_id(), true)],
        1000,
        1,
        Some("ERR_STORAGE_RECIPIENT_OMITTED")
    )]
    #[tokio::test]
    async fn test_storage_deposit_on_fin_transfer(
        build_artifacts: &BuildArtifacts,
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
            build_artifacts,
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
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            network,
            token_contract,
            locker_contract,
            relayer_account,
            token_receiver_contract,
            required_balance_for_fin_transfer,
            ..
        } = setup_contracts(false, is_deployed_token, build_artifacts).await?;

        if !is_deployed_token {
            token_contract
                .call(
                    "ft_transfer",
                    json!({
                        "receiver_id": locker_contract.id,
                        "amount": U128(amount),
                    }),
                    NearToken::from_yoctonear(1),
                    &network,
                )
                .await?;
        }

        let recipient = if msg.is_empty() {
            account_n(1)
        } else {
            let receiver_contract = token_receiver_contract.id.clone();
            storage_deposit_accounts.insert(0, (receiver_contract.clone(), true));
            receiver_contract
        };

        let required_deposit_for_fin_transfer = NEP141_DEPOSIT
            .saturating_mul(u128::try_from(storage_deposit_accounts.len())?)
            .saturating_add(required_balance_for_fin_transfer);

        let storage_deposit_actions = storage_deposit_accounts
            .iter()
            .map(|(account_id, is_deposit_needed)| StorageDepositAction {
                token_id: token_contract.id.clone(),
                account_id: account_id.clone(),
                storage_deposit_amount: is_deposit_needed.then(|| NEP141_DEPOSIT.as_yoctonear()),
            })
            .collect();

        // Fin transfer
        relayer_account
            .call_borsh(
                &locker_contract.id,
                "fin_transfer",
                borsh::to_vec(&FinTransferArgs {
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
                })?,
                required_deposit_for_fin_transfer,
                &network,
            )
            .await?;

        // Check balances
        let recipient_balance: U128 = token_contract
            .view(
                "ft_balance_of",
                json!({
                    "account_id": recipient,
                }),
                &network,
            )
            .await?;
        assert_eq!(expected_recipient_balance, recipient_balance.0);

        let relayer_balance: U128 = token_contract
            .view(
                "ft_balance_of",
                json!({
                    "account_id": relayer_account_id(),
                }),
                &network,
            )
            .await?;
        assert_eq!(expected_relayer_balance, relayer_balance.0);

        let locker_balance: U128 = token_contract
            .view(
                "ft_balance_of",
                json!({
                    "account_id": locker_contract.id
                }),
                &network,
            )
            .await?;
        assert_eq!(expected_locker_balance, locker_balance.0);

        Ok(())
    }

    #[rstest]
    #[case(50)]
    #[case(1_000_000)]
    #[tokio::test]
    async fn test_near_withdrawal(
        build_artifacts: &BuildArtifacts,
        #[case] near_amount: u128,
    ) -> anyhow::Result<()> {
        let TestSetup {
            sandbox,
            network,
            token_contract,
            locker_contract,
            relayer_account,
            required_balance_for_fin_transfer,
            ..
        } = setup_contracts(true, false, build_artifacts).await?;

        // Provide locker contract with large wNEAR balance
        let wnear_amount = NearToken::from_near(near_amount);

        // Get root signer
        let root_signer = Signer::from_secret_key(
            near_sandbox::config::DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY.parse()?,
        )?;

        // top up wNEAR contract with NEAR
        let transfer_result = Tokens::account("test.near".parse()?)
            .send_to(token_contract.id.clone())
            .near(wnear_amount)
            .with_signer(root_signer)
            .send_to(&network)
            .await?;
        assert!(transfer_result.is_success());

        token_contract
            .call(
                "ft_transfer",
                json!({
                    "receiver_id": locker_contract.id,
                    "amount": U128(wnear_amount.as_yoctonear()*2),
                }),
                NearToken::from_yoctonear(1),
                &network,
            )
            .await?;

        let recipient_account = {
            let (secret_key, public_key) = near_sandbox::random_key_pair();
            sandbox
                .create_account(account_n(1))
                .initial_balance(NearToken::from_near(100))
                .public_key(public_key)
                .send()
                .await?;
            let signer = Signer::from_secret_key(secret_key.parse()?)?;
            TestAccount {
                id: account_n(1),
                signer,
            }
        };

        let storage_deposit_actions = vec![StorageDepositAction {
            token_id: token_contract.id.clone(),
            account_id: recipient_account.id.clone(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        }];

        let required_deposit_for_fin_transfer = NEP141_DEPOSIT
            .saturating_mul(u128::try_from(storage_deposit_actions.len())?)
            .saturating_add(required_balance_for_fin_transfer);

        // Try to finalize a large NEAR withdrawal
        let result = relayer_account
            .call_borsh(
                &locker_contract.id,
                "fin_transfer",
                borsh::to_vec(&FinTransferArgs {
                    chain_kind: omni_types::ChainKind::Eth,
                    storage_deposit_actions,
                    prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                        origin_nonce: 1,
                        token: eth_token_address(),
                        recipient: OmniAddress::Near(recipient_account.id.clone()),
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
                })?,
                required_deposit_for_fin_transfer,
                &network,
            )
            .await?;

        assert!(result.is_success(), "Fin transfer failed {result:?}");

        // Check that the NEAR balance of the recipient is greater or equal to the amount
        let account_info = near_api::Account(recipient_account.id.clone())
            .view()
            .fetch_from(&network)
            .await?;
        let recipient_balance = account_info.data.amount;
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
    async fn test_fin_transfer_with_msg(
        build_artifacts: &BuildArtifacts,
        #[case] case: FinTransferWithMsgCase,
    ) {
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
            build_artifacts,
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
    async fn test_fin_transfer_with_msg_for_deployed_token(
        build_artifacts: &BuildArtifacts,
        #[case] case: FinTransferWithMsgCase,
    ) {
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
            build_artifacts,
        )
        .await
        .unwrap();
    }
}
