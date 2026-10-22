#[cfg(test)]
mod tests {
    use near_sdk::{json_types::U128, near, serde_json::json, AccountId};
    use near_workspaces::Contract;
    use rstest::rstest;

    use crate::{
        environment::TestEnvBuilder,
        helpers::tests::{account_n, build_artifacts, BuildArtifacts},
    };

    #[near(serializers=[json])]
    struct TokenReceiverMessage {
        return_value: U128,
        panic: bool,
        extra_msg: String,
    }

    async fn ft_balance_of(
        token_contract: &Contract,
        account_id: &AccountId,
    ) -> anyhow::Result<u128> {
        let balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?;
        Ok(balance.0)
    }

    async fn ft_total_supply(token_contract: &Contract) -> anyhow::Result<u128> {
        let supply: U128 = token_contract.view("ft_total_supply").await?.json()?;
        Ok(supply.0)
    }

    /// The bridge account holds the DAO role in the test environment (granted in `new`).
    #[rstest]
    #[tokio::test]
    async fn mint_token_as_dao_mints_to_recipient(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_bridged_token()
            .await?;
        let recipient = account_n(1);
        env.storage_deposit(&recipient).await?;

        let supply_before = ft_total_supply(&env.token.contract).await?;

        let result = env
            .bridge_contract
            .call("mint_token_as_dao")
            .args_json(json!({
                "token_id": env.token.contract.id(),
                "amount": U128(1000),
                "recipient": recipient,
                "msg": null,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        assert!(result
            .logs()
            .iter()
            .any(|log| log.contains("Minting 1000 of")));
        assert_eq!(1000, ft_balance_of(&env.token.contract, &recipient).await?);
        assert_eq!(
            supply_before + 1000,
            ft_total_supply(&env.token.contract).await?
        );

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn mint_token_as_dao_with_msg_calls_ft_transfer_call(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_bridged_token()
            .await?;
        let receiver = env.deploy_mock_receiver().await?;
        env.storage_deposit(receiver.id()).await?;

        let msg = near_sdk::serde_json::to_string(&TokenReceiverMessage {
            return_value: U128(0),
            panic: false,
            extra_msg: String::new(),
        })?;

        let result = env
            .bridge_contract
            .call("mint_token_as_dao")
            .args_json(json!({
                "token_id": env.token.contract.id(),
                "amount": U128(1000),
                "recipient": receiver.id(),
                "msg": msg,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        assert!(result
            .logs()
            .iter()
            .any(|log| log.contains("ft_on_transfer called with")));
        assert_eq!(
            1000,
            ft_balance_of(&env.token.contract, receiver.id()).await?
        );
        assert_eq!(
            0,
            ft_balance_of(&env.token.contract, env.bridge_contract.id()).await?
        );

        Ok(())
    }

    /// When the receiver refuses the tokens, `ft_transfer_call` refunds them to the sender,
    /// which is the bridge account. The DAO can then remove them with `burn_bridge_balance`.
    #[rstest]
    #[tokio::test]
    async fn mint_token_as_dao_refused_transfer_call_is_burnable(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_bridged_token()
            .await?;
        let receiver = env.deploy_mock_receiver().await?;
        env.storage_deposit(receiver.id()).await?;

        let supply_before = ft_total_supply(&env.token.contract).await?;
        let msg = near_sdk::serde_json::to_string(&TokenReceiverMessage {
            return_value: U128(1000),
            panic: false,
            extra_msg: String::new(),
        })?;

        env.bridge_contract
            .call("mint_token_as_dao")
            .args_json(json!({
                "token_id": env.token.contract.id(),
                "amount": U128(1000),
                "recipient": receiver.id(),
                "msg": msg,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        assert_eq!(0, ft_balance_of(&env.token.contract, receiver.id()).await?);
        assert_eq!(
            1000,
            ft_balance_of(&env.token.contract, env.bridge_contract.id()).await?
        );

        env.bridge_contract
            .call("burn_bridge_balance")
            .args_json(json!({
                "token_id": env.token.contract.id(),
                "amount": U128(1000),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        assert_eq!(
            0,
            ft_balance_of(&env.token.contract, env.bridge_contract.id()).await?
        );
        assert_eq!(supply_before, ft_total_supply(&env.token.contract).await?);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn mint_token_as_dao_non_dao_caller_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_bridged_token()
            .await?;
        let caller = env.create_account(account_n(1)).await?;
        env.storage_deposit(caller.id()).await?;

        let err = caller
            .call(env.bridge_contract.id(), "mint_token_as_dao")
            .args_json(json!({
                "token_id": env.token.contract.id(),
                "amount": U128(1000),
                "recipient": caller.id(),
                "msg": null,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()
            .expect_err("non-DAO caller must be rejected")
            .to_string();

        assert!(
            err.contains("Insufficient permissions"),
            "unexpected error: {err}"
        );
        assert_eq!(0, ft_balance_of(&env.token.contract, caller.id()).await?);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn mint_token_as_dao_non_deployed_token_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_native_nep141_token(24)
            .await?;

        let err = env
            .bridge_contract
            .call("mint_token_as_dao")
            .args_json(json!({
                "token_id": env.token.contract.id(),
                "amount": U128(1000),
                "recipient": account_n(1),
                "msg": null,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()
            .expect_err("token that is not deployed by the bridge must be rejected")
            .to_string();

        assert!(
            err.contains("ERR_TOKEN_NOT_FOUND"),
            "unexpected error: {err}"
        );

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn mint_token_as_dao_unregistered_recipient_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_bridged_token()
            .await?;
        let supply_before = ft_total_supply(&env.token.contract).await?;

        let result = env
            .bridge_contract
            .call("mint_token_as_dao")
            .args_json(json!({
                "token_id": env.token.contract.id(),
                "amount": U128(1000),
                "recipient": account_n(1),
                "msg": null,
            }))
            .max_gas()
            .transact()
            .await?;

        assert!(
            result.is_failure(),
            "mint to an unregistered account must fail"
        );
        assert_eq!(supply_before, ft_total_supply(&env.token.contract).await?);

        Ok(())
    }
}
