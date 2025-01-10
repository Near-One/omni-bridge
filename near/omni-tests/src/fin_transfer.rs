#[cfg(test)]
mod tests {
    use crate::helpers::tests::{
        account_n, eth_eoa_address, eth_factory_address, eth_token_address, relayer_account_id,
        LOCKER_PATH, MOCK_PROVER_PATH, MOCK_TOKEN_PATH, NEP141_DEPOSIT,
    };
    use near_sdk::{borsh, json_types::U128, serde_json::json, AccountId};
    use near_workspaces::types::NearToken;
    use omni_types::{
        locker_args::{BindTokenArgs, FinTransferArgs, StorageDepositAction},
        prover_result::{DeployTokenMessage, InitTransferMessage, ProverResult},
        Fee, OmniAddress,
    };
    use rstest::rstest;

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
        let result = test_fin_transfer(storage_deposit_accounts, amount, fee).await;

        match result {
            Ok(_) => assert!(
                expected_error.is_none(),
                "Expected an error but got success"
            ),
            Err(result_error) => {
                let error = expected_error.expect(&format!(
                    "Got an error {result_error} when none was expected"
                ));
                assert!(
                    result_error.to_string().contains(error),
                    "Wrong error. Got: {}, Expected: {}",
                    result_error,
                    error
                );
            }
        }
    }

    async fn test_fin_transfer(
        storage_deposit_accounts: Vec<(AccountId, bool)>,
        amount: u128,
        fee: u128,
    ) -> anyhow::Result<()> {
        let worker = near_workspaces::sandbox().await?;
        // Deploy and init FT token
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
        // Deploy and init locker
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

        // Get required balances
        let required_balance_for_fin_transfer: NearToken = locker_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;

        // Create relayer account
        let relayer_account = worker
            .create_tla(relayer_account_id(), worker.dev_generate().await.1)
            .await?
            .unwrap();

        // Storage deposit and transfer tokens
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
                "amount": U128(amount),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
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

        let required_deposit_for_fin_transfer = NEP141_DEPOSIT
            .saturating_mul(storage_deposit_accounts.len() as u128)
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
                    recipient: OmniAddress::Near(account_n(1)),
                    amount: U128(amount),
                    fee: Fee {
                        fee: U128(fee),
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
            .await?
            .into_result()?;

        // Check balances
        let recipient_balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": account_n(1),
            }))
            .await?
            .json()?;
        assert_eq!(amount - fee, recipient_balance.0);

        let relayer_balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": relayer_account_id(),
            }))
            .await?
            .json()?;
        assert_eq!(fee, relayer_balance.0);

        Ok(())
    }
}
