#[cfg(test)]
mod tests {
    use crate::helpers::tests::{
        account_1, account_2, eth_eoa_address, eth_factory_address, relayer_account_id,
        LOCKER_PATH, MOCK_PROVER_PATH, MOCK_TOKEN_PATH, NEP141_DEPOSIT,
    };
    use near_sdk::{borsh, json_types::U128, serde_json::json, AccountId};
    use near_workspaces::types::NearToken;
    use omni_types::{
        locker_args::{FinTransferArgs, StorageDepositAction},
        prover_result::{InitTransferMessage, ProverResult},
        Fee, OmniAddress,
    };

    #[tokio::test]
    async fn test_storage_deposit_on_fin_transfer() {
        struct TestStorageDeposit<'a> {
            storage_deposit_accounts: Vec<(AccountId, bool)>,
            amount: u128,
            fee: u128,
            error: Option<&'a str>,
        }
        let test_data = [
            TestStorageDeposit {
                storage_deposit_accounts: [(account_1(), true), (relayer_account_id(), true)]
                    .to_vec(),
                amount: 1000,
                fee: 1,
                error: None,
            },
            TestStorageDeposit {
                storage_deposit_accounts: [(account_1(), true)].to_vec(),
                amount: 1000,
                fee: 0,
                error: None,
            },
            TestStorageDeposit {
                storage_deposit_accounts: [
                    (account_1(), true),
                    (relayer_account_id(), true),
                    (account_2(), true),
                    (account_2(), true),
                ]
                .to_vec(),
                amount: 1000,
                fee: 1,
                error: Some("Invalid len of accounts for storage deposit"),
            },
            TestStorageDeposit {
                storage_deposit_accounts: [(relayer_account_id(), true), (account_1(), true)]
                    .to_vec(),
                amount: 1000,
                fee: 1,
                error: Some("STORAGE_ERR: The transfer recipient is omitted"),
            },
            TestStorageDeposit {
                storage_deposit_accounts: [(account_1(), true)].to_vec(),
                amount: 1000,
                fee: 1,
                error: Some("STORAGE_ERR: The fee recipient is omitted"),
            },
            TestStorageDeposit {
                storage_deposit_accounts: [].to_vec(),
                amount: 1000,
                fee: 1,
                error: Some("STORAGE_ERR: The transfer recipient is omitted"),
            },
            TestStorageDeposit {
                storage_deposit_accounts: [(account_1(), false), (relayer_account_id(), false)]
                    .to_vec(),
                amount: 1000,
                fee: 1,
                error: Some("STORAGE_ERR: The transfer recipient is omitted"),
            },
            TestStorageDeposit {
                storage_deposit_accounts: [(account_1(), true), (relayer_account_id(), false)]
                    .to_vec(),
                amount: 1000,
                fee: 1,
                error: Some("STORAGE_ERR: The fee recipient is omitted"),
            },
            TestStorageDeposit {
                storage_deposit_accounts: [(account_1(), false), (relayer_account_id(), true)]
                    .to_vec(),
                amount: 1000,
                fee: 1,
                error: Some("STORAGE_ERR: The transfer recipient is omitted"),
            },
        ];

        for (index, test) in test_data.into_iter().enumerate() {
            let result =
                test_fin_transfer(test.storage_deposit_accounts, test.amount, test.fee).await;

            match result {
                Ok(_) => assert!(test.error.is_none()),
                Err(result_error) => match test.error {
                    Some(exepected_error) => {
                        assert!(
                            result_error.to_string().contains(exepected_error),
                            "Wrong error. Test index: {}, err: {}, expected: {}",
                            index,
                            result_error,
                            exepected_error
                        )
                    }
                    None => panic!("Test index: {}, err: {}", index, result_error),
                },
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
                    token: OmniAddress::Near(token_contract.id().clone()),
                    recipient: OmniAddress::Near(account_1()),
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
                "account_id": account_1(),
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
