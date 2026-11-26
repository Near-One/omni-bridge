#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json, Value},
    };
    use near_workspaces::{
        network::Sandbox,
        result::{ExecutionFailure, ExecutionSuccess},
        types::{AccessKeyPermission, Gas, KeyType, NearToken, PublicKey, SecretKey},
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

        async fn with_all_roles(factory_wasm: &[u8]) -> anyhow::Result<Self> {
            let roles = ["DAO", "TokenDeployer", "TokenDepositer", "OmniProver"];
            Self::new(factory_wasm, &roles, &roles).await
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

        async fn record_withdrawal(
            &self,
            caller: &Account,
            withdrawal_id: &str,
            withdrawal: &Value,
        ) -> anyhow::Result<Result<ExecutionSuccess, ExecutionFailure>> {
            Ok(caller
                .call(self.factory.id(), "record_withdrawal")
                .args_json(json!({
                    "withdrawal_id": withdrawal_id,
                    "withdrawal": withdrawal,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .gas(Gas::from_tgas(30))
                .transact()
                .await?
                .into_result())
        }

        async fn update_withdrawal(
            &self,
            caller: &Account,
            withdrawal_id: &str,
            prev_payload_hash: &str,
            new_payload_hash: &str,
            metadata: &str,
        ) -> anyhow::Result<Result<ExecutionSuccess, ExecutionFailure>> {
            Ok(caller
                .call(self.factory.id(), "update_withdrawal")
                .args_json(json!({
                    "withdrawal_id": withdrawal_id,
                    "prev_payload_hash": prev_payload_hash,
                    "new_payload_hash": new_payload_hash,
                    "metadata": metadata,
                }))
                .deposit(NearToken::from_yoctonear(4))
                .gas(Gas::from_tgas(30))
                .transact()
                .await?
                .into_result())
        }

        async fn get_withdrawal(&self, withdrawal_id: &str) -> anyhow::Result<Value> {
            Ok(self
                .factory
                .view("get_withdrawal")
                .args_json(json!({ "withdrawal_id": withdrawal_id }))
                .await?
                .json()?)
        }

        async fn manage_token_keys(
            &self,
            caller: &Account,
            method: &str,
            public_key: &PublicKey,
            tokens: &[&str],
        ) -> anyhow::Result<Result<ExecutionSuccess, ExecutionFailure>> {
            Ok(caller
                .call(self.factory.id(), method)
                .args_json(json!({
                    "public_key": public_key,
                    "tokens": tokens,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .gas(Gas::from_tgas(100))
                .transact()
                .await?
                .into_result())
        }

        async fn full_access_keys(&self, account_id: &AccountId) -> anyhow::Result<Vec<PublicKey>> {
            Ok(self
                .worker
                .view_access_keys(account_id)
                .await?
                .into_iter()
                .filter(|key| matches!(key.access_key.permission, AccessKeyPermission::FullAccess))
                .map(|key| key.public_key)
                .collect())
        }
    }

    /// Callers digest their own ids, so tests just pick distinct digests.
    fn id(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    fn payload_hash(byte: u8) -> String {
        hex::encode([byte; 32])
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

    #[rstest]
    #[tokio::test]
    async fn test_record_withdrawal_rejects_duplicate(
        intents_bridge_factory_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::with_all_roles(&intents_bridge_factory_wasm).await?;
        let unauthorized = env.create_account("unauth").await?;

        let withdrawal = json!({ "payload_hash": payload_hash(1), "metadata": "meta-1" });
        let w1 = id(1);

        assert!(env
            .record_withdrawal(&unauthorized, &w1, &withdrawal)
            .await?
            .is_err());

        env.record_withdrawal(&env.root, &w1, &withdrawal).await??;
        assert_eq!(env.get_withdrawal(&w1).await?, withdrawal);

        assert_failure_contains(
            env.record_withdrawal(&env.root, &w1, &withdrawal).await?,
            "withdrawal already exists",
        );

        env.root
            .call(env.factory.id(), "remove_withdrawals")
            .args_json(json!({ "withdrawal_ids": [w1] }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(Gas::from_tgas(30))
            .transact()
            .await?
            .into_result()?;
        assert_eq!(env.get_withdrawal(&w1).await?, Value::Null);
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_update_withdrawal(intents_bridge_factory_wasm: Vec<u8>) -> anyhow::Result<()> {
        let env = TestEnv::with_all_roles(&intents_bridge_factory_wasm).await?;
        let unauthorized = env.create_account("unauth-upd").await?;

        let w_upd = id(2);
        let prev_hash = payload_hash(9);
        env.record_withdrawal(
            &env.root,
            &w_upd,
            &json!({ "payload_hash": prev_hash, "metadata": "meta-orig" }),
        )
        .await??;

        let updated_hash = payload_hash(5);
        let updated_metadata = "meta-updated";

        assert!(env
            .update_withdrawal(
                &unauthorized,
                &w_upd,
                &prev_hash,
                &updated_hash,
                updated_metadata
            )
            .await?
            .is_err());

        assert_failure_contains(
            env.update_withdrawal(
                &env.root,
                &w_upd,
                &payload_hash(0),
                &updated_hash,
                updated_metadata,
            )
            .await?,
            "payload hash mismatch",
        );

        assert_failure_contains(
            env.update_withdrawal(
                &env.root,
                &id(0xEE),
                &prev_hash,
                &updated_hash,
                updated_metadata,
            )
            .await?,
            "withdrawal not found",
        );

        env.update_withdrawal(
            &env.root,
            &w_upd,
            &prev_hash,
            &updated_hash,
            updated_metadata,
        )
        .await??;

        assert_eq!(
            env.get_withdrawal(&w_upd).await?,
            json!({ "payload_hash": updated_hash, "metadata": updated_metadata })
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_add_full_access_key_to_tokens(
        intents_bridge_factory_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(
            &intents_bridge_factory_wasm,
            &["DAO"],
            &["DAO", "TokenDeployer"],
        )
        .await?;

        let tokens = ["ft1", "ft2", "ft3"];
        for token in tokens {
            env.deploy_token(&env.root, token).await??;
            assert!(env.full_access_keys(&env.token_id(token)).await?.is_empty());
        }

        let public_key = SecretKey::from_random(KeyType::ED25519).public_key();
        let dao = env.create_account("dao").await?;
        env.root
            .call(env.factory.id(), "acl_grant_role")
            .args_json(json!({ "role": "DAO", "account_id": dao.id() }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        env.manage_token_keys(&dao, "add_full_access_key_to_tokens", &public_key, &tokens)
            .await??;

        // The factory detaches its promises, so verify the resulting keys on every token.
        for token in tokens {
            assert_eq!(
                env.full_access_keys(&env.token_id(token)).await?,
                vec![public_key.clone()]
            );
        }
        assert!(!env
            .full_access_keys(env.factory.id())
            .await?
            .contains(&public_key));

        env.manage_token_keys(&dao, "delete_key_from_tokens", &public_key, &tokens)
            .await??;

        for token in tokens {
            assert!(env.full_access_keys(&env.token_id(token)).await?.is_empty());
        }
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_add_full_access_key_to_tokens_requires_deployed_tokens(
        intents_bridge_factory_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(
            &intents_bridge_factory_wasm,
            &["DAO"],
            &["DAO", "TokenDeployer"],
        )
        .await?;
        env.deploy_token(&env.root, "ft1").await??;

        let undeployed_token = "missing";
        assert_failure_contains(
            env.manage_token_keys(
                &env.root,
                "add_full_access_key_to_tokens",
                &SecretKey::from_random(KeyType::ED25519).public_key(),
                &["ft1", undeployed_token],
            )
            .await?,
            &format!("token '{undeployed_token}' is not deployed"),
        );

        assert!(env.full_access_keys(&env.token_id("ft1")).await?.is_empty());
        Ok(())
    }
}
