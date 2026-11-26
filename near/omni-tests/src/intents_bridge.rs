#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json, Value},
    };
    use near_workspaces::{
        network::Sandbox,
        result::{ExecutionFailure, ExecutionSuccess},
        types::{Gas, NearToken},
        Account, AccountId, Contract, Worker,
    };
    use rstest::rstest;

    use crate::helpers::tests::intents_bridge_factory_wasm;

    const TOKEN_INIT_BALANCE: NearToken = NearToken::from_near(3);
    const FT_DEPOSIT_ATTACHMENT: NearToken = NearToken::from_millinear(4);

    struct TestEnv {
        worker: Worker<Sandbox>,
        root: Account,
        factory: Contract,
    }

    impl TestEnv {
        async fn new(
            factory_wasm: &[u8],
            admin_roles: &[&str],
            grantee_roles: &[&str],
        ) -> anyhow::Result<Self> {
            let worker = near_workspaces::sandbox().await?;
            let root = worker.root_account()?;

            let factory = root
                .create_subaccount("factory")
                .initial_balance(NearToken::from_near(50))
                .transact()
                .await?
                .into_result()?
                .deploy(factory_wasm)
                .await?
                .into_result()?;

            let roles_for_root = |roles: &[&str]| -> Value {
                roles
                    .iter()
                    .map(|role| ((*role).to_string(), json!([root.id()])))
                    .collect::<serde_json::Map<_, _>>()
                    .into()
            };

            factory
                .call("new")
                .args_json(json!({
                    "super_admins": [root.id()],
                    "admins": roles_for_root(admin_roles),
                    "grantees": roles_for_root(grantee_roles),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(Self {
                worker,
                root,
                factory,
            })
        }

        async fn create_account(&self, name: &str) -> anyhow::Result<Account> {
            Ok(self
                .root
                .create_subaccount(name)
                .initial_balance(NearToken::from_near(10))
                .transact()
                .await?
                .into_result()?)
        }

        fn token_id(&self, token: &str) -> AccountId {
            format!("{token}.{}", self.factory.id()).parse().unwrap()
        }

        async fn deploy_token(
            &self,
            caller: &Account,
            token: &str,
        ) -> anyhow::Result<Result<ExecutionSuccess, ExecutionFailure>> {
            Ok(caller
                .call(self.factory.id(), "deploy_token")
                .args_json(json!({ "token": token }))
                .deposit(TOKEN_INIT_BALANCE)
                .gas(Gas::from_tgas(300))
                .transact()
                .await?
                .into_result())
        }

        async fn ft_deposit(
            &self,
            caller: &Account,
            token: &str,
            owner_id: &AccountId,
            amount: u128,
        ) -> anyhow::Result<Result<ExecutionSuccess, ExecutionFailure>> {
            Ok(caller
                .call(self.factory.id(), "ft_deposit")
                .args_json(json!({
                    "token": token,
                    "owner_id": owner_id,
                    "amount": U128(amount),
                }))
                .deposit(FT_DEPOSIT_ATTACHMENT)
                .gas(Gas::from_tgas(300))
                .transact()
                .await?
                .into_result())
        }
    }

    fn assert_failure_contains(result: Result<ExecutionSuccess, ExecutionFailure>, expected: &str) {
        let err = result.expect_err("call should fail");
        assert!(
            format!("{err:?}").contains(expected),
            "expected error containing {expected:?}, got: {err:?}"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_deploy_token_and_mint(
        intents_bridge_factory_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let roles = ["TokenDeployer", "TokenDepositer"];
        let env = TestEnv::new(&intents_bridge_factory_wasm, &roles, &roles).await?;
        let user = env.create_account("user1").await?;

        assert!(env.deploy_token(&user, "ft1").await?.is_err());
        assert_failure_contains(
            env.deploy_token(&env.root, "ft1.abc").await?,
            "invalid token name",
        );
        env.deploy_token(&env.root, "ft1").await??;
        assert_failure_contains(env.deploy_token(&env.root, "ft1").await?, "token exists");

        let ft1 = env.token_id("ft1");
        for account_id in [env.root.id(), user.id()] {
            env.root
                .call(&ft1, "storage_deposit")
                .args_json(json!({ "account_id": account_id }))
                .deposit(NearToken::from_near(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;
        }

        let balance_of = |account_id: AccountId| {
            let ft1 = ft1.clone();
            let worker = env.worker.clone();
            async move {
                worker
                    .view(&ft1, "ft_balance_of")
                    .args_json(json!({ "account_id": account_id }))
                    .await?
                    .json::<U128>()
            }
        };
        assert_eq!(balance_of(user.id().clone()).await?, U128(0));

        assert!(env
            .ft_deposit(&user, "ft1", user.id(), 1000)
            .await?
            .is_err());
        env.ft_deposit(&env.root, "ft1", user.id(), 1000).await??;

        assert_eq!(balance_of(user.id().clone()).await?, U128(1000));
        Ok(())
    }
}
