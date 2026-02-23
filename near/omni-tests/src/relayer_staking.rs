#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
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
    async fn test_apply_claim_relayer_role(
        #[from(locker_wasm)] locker: Vec<u8>,
        #[from(mock_prover_wasm)] prover: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(locker, prover).await?;

        // Set a short waiting period for testing (1 second in nanoseconds)
        env.bridge_contract
            .call("set_relayer_config")
            .args_json(json!({
                "stake_required": U128(1_000 * 10u128.pow(24)),
                "waiting_period_ns": 1_000_000_000u64,
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

        // Fast forward past waiting period
        env.worker.fast_forward(100).await?;

        // Claim
        let result = applicant
            .call(env.bridge_contract.id(), "claim_trusted_relayer_role")
            .max_gas()
            .transact()
            .await?;
        result.into_result()?;

        // Verify role is granted
        let has_role: bool = env
            .bridge_contract
            .view("acl_has_role")
            .args_json(json!({"role": "TrustedRelayer", "account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(has_role);

        // Verify stake is stored
        let stake: Option<U128> = env
            .bridge_contract
            .view("get_relayer_stake")
            .args_json(json!({"account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(stake.is_some());
        assert!(stake.unwrap().0 >= 1_000 * 10u128.pow(24));

        // Verify application is removed
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
    async fn test_claim_before_waiting_period(
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

        // Attempt to claim immediately (before waiting period)
        let result = applicant
            .call(env.bridge_contract.id(), "claim_trusted_relayer_role")
            .max_gas()
            .transact()
            .await?;

        assert!(result.into_result().is_err());

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

        // DAO rejects (bridge_contract is the deployer which is super admin / DAO)
        env.bridge_contract
            .call("reject_relayer_application")
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

        // Verify NEAR was returned (balance should have recovered)
        let balance_after_reject = applicant.view_account().await?.balance;
        assert!(balance_after_reject.as_yoctonear() > balance_after_apply.as_yoctonear());

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
                "waiting_period_ns": 1_000_000_000u64,
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

        // Wait and claim
        env.worker.fast_forward(100).await?;

        applicant
            .call(env.bridge_contract.id(), "claim_trusted_relayer_role")
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Verify role
        let has_role: bool = env
            .bridge_contract
            .view("acl_has_role")
            .args_json(json!({"role": "TrustedRelayer", "account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(has_role);

        let balance_before_resign = applicant.view_account().await?.balance;

        // Resign
        applicant
            .call(env.bridge_contract.id(), "resign_trusted_relayer")
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Verify role is revoked
        let has_role: bool = env
            .bridge_contract
            .view("acl_has_role")
            .args_json(json!({"role": "TrustedRelayer", "account_id": applicant.id()}))
            .await?
            .json()?;
        assert!(!has_role);

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
        assert_eq!(config["waiting_period_ns"], json!(604_800_000_000_000u64));

        // DAO updates config
        env.bridge_contract
            .call("set_relayer_config")
            .args_json(json!({
                "stake_required": U128(500 * 10u128.pow(24)),
                "waiting_period_ns": 86_400_000_000_000u64,
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
        assert_eq!(config["waiting_period_ns"], json!(86_400_000_000_000u64));

        Ok(())
    }
}
