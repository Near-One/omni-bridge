#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::{U128, U64},
        serde_json::{self, json},
    };
    use near_workspaces::types::NearToken;
    use rstest::rstest;

    use crate::helpers::tests::{locker_wasm, mock_prover_wasm};

    struct TestEnv {
        worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
        bridge_contract: near_workspaces::Contract,
    }

    impl TestEnv {
        async fn new(locker_wasm: Vec<u8>, mock_prover_wasm: Vec<u8>) -> anyhow::Result<Self> {
            let worker = near_workspaces::sandbox().await?;

            let bridge_contract = worker.dev_deploy(&locker_wasm).await?;

            bridge_contract
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

            let prover = worker.dev_deploy(&mock_prover_wasm).await?;
            bridge_contract
                .call("add_prover")
                .args_json(json!({
                    "chain": "Eth",
                    "account_id": prover.id(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(Self {
                worker,
                bridge_contract,
            })
        }

        async fn create_funded_account(
            &self,
            name: &str,
            near_amount: u128,
        ) -> anyhow::Result<near_workspaces::Account> {
            let account = self
                .worker
                .create_tla(
                    name.parse().unwrap(),
                    self.worker.generate_dev_account_credentials().1,
                )
                .await?
                .unwrap();

            if near_amount > 0 {
                self.worker
                    .root_account()?
                    .transfer_near(account.id(), NearToken::from_near(near_amount))
                    .await?
                    .into_result()?;
            }

            Ok(account)
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_apply_auto_promote_relayer(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        // Set a short waiting period for testing (1 second in nanoseconds)
        env.bridge_contract
            .call("set_relayer_config")
            .args_json(json!({
                "stake_required": U128(1_000 * 10u128.pow(24)),
                "waiting_period_ns": U64(1_000_000_000),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let applicant = env.create_funded_account("applicant", 2000).await?;

        // Apply
        let result = applicant
            .call(env.bridge_contract.id(), "apply_for_trusted_relayer")
            .deposit(NearToken::from_near(1000))
            .max_gas()
            .transact()
            .await?;
        result.into_result()?;

        // Verify application exists
        let application: Option<serde_json::Value> = env
            .bridge_contract
            .view("get_relayer_application")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(application.is_some());

        // Before waiting period, relayer should not be trusted
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(!is_trusted);

        // Fast forward past waiting period
        env.worker.fast_forward(100).await?;

        // After waiting period, relayer should be trusted
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(is_trusted);

        // Verify stake is stored
        let stake: Option<U128> = env
            .bridge_contract
            .view("get_relayer_stake")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(stake.is_some());
        assert!(stake.unwrap().0 >= 1_000 * 10u128.pow(24));

        // Verify application is no longer pending
        let application: Option<serde_json::Value> = env
            .bridge_contract
            .view("get_relayer_application")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(application.is_none());

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_apply_insufficient_stake(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        let applicant = env.create_funded_account("applicant", 2000).await?;

        // Apply with insufficient stake (999 NEAR)
        let result = applicant
            .call(env.bridge_contract.id(), "apply_for_trusted_relayer")
            .deposit(NearToken::from_near(999))
            .max_gas()
            .transact()
            .await?;

        assert!(result.into_result().is_err());

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_not_trusted_before_waiting_period(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        let applicant = env.create_funded_account("applicant", 2000).await?;

        // Apply
        applicant
            .call(env.bridge_contract.id(), "apply_for_trusted_relayer")
            .deposit(NearToken::from_near(1000))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Relayer should not be trusted before waiting period elapses
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(!is_trusted);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_dao_reject_application(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        let applicant = env.create_funded_account("applicant", 2000).await?;

        let balance_before = applicant.view_account().await?.balance;

        // Apply
        applicant
            .call(env.bridge_contract.id(), "apply_for_trusted_relayer")
            .deposit(NearToken::from_near(1000))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let balance_after_apply = applicant.view_account().await?.balance;
        assert!(
            balance_before.as_yoctonear() - balance_after_apply.as_yoctonear()
                >= NearToken::from_near(1000).as_yoctonear()
        );

        // Create a separate DAO account and grant it the DAO role
        let dao_account = env.create_funded_account("dao-account", 10).await?;
        env.bridge_contract
            .call("acl_grant_role")
            .args_json(json!({
                "role": "DAO",
                "account_id": dao_account.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // DAO rejects
        let dao_balance_before_reject = dao_account.view_account().await?.balance;
        dao_account
            .call(env.bridge_contract.id(), "reject_relayer_application")
            .args_json(json!({"account_id": applicant.id()}))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Verify application is removed
        let application: Option<serde_json::Value> = env
            .bridge_contract
            .view("get_relayer_application")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(application.is_none());

        // Verify stake was NOT returned to applicant (goes to DAO/relayer manager)
        let balance_after_reject = applicant.view_account().await?.balance;
        assert!(balance_after_reject.as_yoctonear() <= balance_after_apply.as_yoctonear());

        // Verify stake was transferred to DAO account
        let dao_balance_after_reject = dao_account.view_account().await?.balance;
        assert!(dao_balance_after_reject.as_yoctonear() > dao_balance_before_reject.as_yoctonear());

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_resign_relayer(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        // Set a short waiting period
        env.bridge_contract
            .call("set_relayer_config")
            .args_json(json!({
                "stake_required": U128(1_000 * 10u128.pow(24)),
                "waiting_period_ns": U64(1_000_000_000),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let applicant = env.create_funded_account("applicant", 2000).await?;

        // Apply
        applicant
            .call(env.bridge_contract.id(), "apply_for_trusted_relayer")
            .deposit(NearToken::from_near(1000))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Wait past activation period
        env.worker.fast_forward(100).await?;

        // Verify relayer is now trusted
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(is_trusted);

        let balance_before_resign = applicant.view_account().await?.balance;

        // Resign
        applicant
            .call(env.bridge_contract.id(), "resign_trusted_relayer")
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Verify relayer is no longer trusted
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(!is_trusted);

        // Verify NEAR was returned
        let balance_after_resign = applicant.view_account().await?.balance;
        assert!(balance_after_resign.as_yoctonear() > balance_before_resign.as_yoctonear());

        // Verify stake is removed
        let stake: Option<U128> = env
            .bridge_contract
            .view("get_relayer_stake")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(stake.is_none());

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_resign_non_active_relayer_fails(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        let applicant = env.create_funded_account("applicant", 2000).await?;

        // Apply
        applicant
            .call(env.bridge_contract.id(), "apply_for_trusted_relayer")
            .deposit(NearToken::from_near(1000))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Try to resign before activation (should fail)
        let result = applicant
            .call(env.bridge_contract.id(), "resign_trusted_relayer")
            .max_gas()
            .transact()
            .await?;

        assert!(result.into_result().is_err());

        // Verify the relayer application still exists
        let application: Option<serde_json::Value> = env
            .bridge_contract
            .view("get_relayer_application")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(application.is_some());

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_dao_revoke_active_relayer(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        // Set a short waiting period
        env.bridge_contract
            .call("set_relayer_config")
            .args_json(json!({
                "stake_required": U128(1_000 * 10u128.pow(24)),
                "waiting_period_ns": U64(1_000_000_000),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let applicant = env.create_funded_account("applicant", 2000).await?;

        // Apply
        applicant
            .call(env.bridge_contract.id(), "apply_for_trusted_relayer")
            .deposit(NearToken::from_near(1000))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Wait past activation period
        env.worker.fast_forward(100).await?;

        // Verify relayer is now trusted
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(is_trusted);

        // Create a DAO account and grant it the DAO role
        let dao_account = env.create_funded_account("dao-account", 10).await?;
        env.bridge_contract
            .call("acl_grant_role")
            .args_json(json!({
                "role": "DAO",
                "account_id": dao_account.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // DAO revokes active relayer
        let dao_balance_before = dao_account.view_account().await?.balance;
        dao_account
            .call(env.bridge_contract.id(), "reject_relayer_application")
            .args_json(json!({"account_id": applicant.id()}))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Verify relayer is no longer trusted
        let is_trusted: bool = env
            .bridge_contract
            .view("is_trusted_relayer")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(!is_trusted);

        // Verify stake was transferred to DAO account
        let dao_balance_after = dao_account.view_account().await?.balance;
        assert!(dao_balance_after.as_yoctonear() > dao_balance_before.as_yoctonear());

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_set_relayer_config(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        // Verify defaults
        let config: serde_json::Value = env
            .bridge_contract
            .view("get_relayer_config")
            .await?
            .json()?;
        let default_stake = (1_000u128 * 10u128.pow(24)).to_string();
        assert_eq!(config["stake_required"], json!(default_stake));
        assert_eq!(config["waiting_period_ns"], json!(U64(604_800_000_000_000)));

        // DAO updates config
        env.bridge_contract
            .call("set_relayer_config")
            .args_json(json!({
                "stake_required": U128(500 * 10u128.pow(24)),
                "waiting_period_ns": U64(86_400_000_000_000),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Verify updated
        let config: serde_json::Value = env
            .bridge_contract
            .view("get_relayer_config")
            .await?
            .json()?;
        let updated_stake = (500u128 * 10u128.pow(24)).to_string();
        assert_eq!(config["stake_required"], json!(updated_stake));
        assert_eq!(config["waiting_period_ns"], json!(U64(86_400_000_000_000)));

        Ok(())
    }
}
