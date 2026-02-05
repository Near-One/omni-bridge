#[cfg(test)]
mod tests {
    use near_api::{AccountId, Contract as ApiContract, NetworkConfig, Signer};
    use near_sandbox::Sandbox;
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
    };
    use near_token::NearToken;
    use omni_types::{near_events::OmniBridgeEvent, InitTransferMsg, OmniAddress, TransferMessage};
    use rstest::rstest;

    use crate::helpers::tests::{
        account_n, eth_eoa_address, eth_factory_address, get_event_data, locker_wasm,
        mock_prover_wasm, mock_token_wasm, NEP141_DEPOSIT,
    };

    use crate::environment::{TestAccount, TestContract};

    struct TestEnv {
        sandbox: Sandbox,
        network: NetworkConfig,
        token_contract: TestContract,
        locker_contract: TestContract,
        sender_account: TestAccount,
    }

    impl TestEnv {
        #[allow(clippy::too_many_lines)]
        async fn new(
            mock_token_wasm: Vec<u8>,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);

            let sandbox = Sandbox::start_sandbox().await?;
            let rpc_url: url::Url = sandbox.rpc_addr.parse()?;
            let network = NetworkConfig::from_rpc_url("sandbox", rpc_url);

            // Deploy and initialize FT token
            let token_contract = {
                let contract_id: AccountId = format!("dev-{}.test.near", COUNTER.fetch_add(1, Ordering::SeqCst)).parse()?;
                let (secret_key, public_key) = near_sandbox::random_key_pair();
                sandbox.create_account(contract_id.clone())
                    .initial_balance(NearToken::from_near(50))
                    .public_key(public_key)
                    .send()
                    .await?;
                let signer = Signer::from_secret_key(secret_key.parse()?)?;
                ApiContract::deploy(contract_id.clone())
                    .use_code(mock_token_wasm.clone())
                    .without_init_call()
                    .with_signer(signer.clone())
                    .send_to(&network)
                    .await?;
                TestContract { id: contract_id, signer }
            };
            token_contract
                .call(
                    "new_default_meta",
                    json!({
                        "owner_id": token_contract.id,
                        "total_supply": U128(u128::MAX)
                    }),
                    NearToken::from_yoctonear(0),
                    &network,
                )
                .await?;

            // Deploy and initialize locker
            let locker_contract = {
                let contract_id: AccountId = format!("dev-{}.test.near", COUNTER.fetch_add(1, Ordering::SeqCst)).parse()?;
                let (secret_key, public_key) = near_sandbox::random_key_pair();
                sandbox.create_account(contract_id.clone())
                    .initial_balance(NearToken::from_near(50))
                    .public_key(public_key)
                    .send()
                    .await?;
                let signer = Signer::from_secret_key(secret_key.parse()?)?;
                ApiContract::deploy(contract_id.clone())
                    .use_code(locker_wasm.clone())
                    .without_init_call()
                    .with_signer(signer.clone())
                    .send_to(&network)
                    .await?;
                TestContract { id: contract_id, signer }
            };
            locker_contract
                .call(
                    "new",
                    json!({
                        "mpc_signer": "mpc.testnet",
                        "nonce": U128(0),
                        "wnear_account_id": "wnear.testnet",
                        "btc_connector": "brg-dev.testnet",
                    }),
                    NearToken::from_yoctonear(0),
                    &network,
                )
                .await?;

            let prover = {
                let contract_id: AccountId = format!("dev-{}.test.near", COUNTER.fetch_add(1, Ordering::SeqCst)).parse()?;
                let (secret_key, public_key) = near_sandbox::random_key_pair();
                sandbox.create_account(contract_id.clone())
                    .initial_balance(NearToken::from_near(50))
                    .public_key(public_key)
                    .send()
                    .await?;
                let signer = Signer::from_secret_key(secret_key.parse()?)?;
                ApiContract::deploy(contract_id.clone())
                    .use_code(mock_prover_wasm.clone())
                    .without_init_call()
                    .with_signer(signer.clone())
                    .send_to(&network)
                    .await?;
                TestContract { id: contract_id, signer }
            };
            locker_contract
                .call(
                    "add_prover",
                    json!({
                        "chain": "Eth",
                        "account_id": prover.id,
                    }),
                    NearToken::from_yoctonear(0),
                    &network,
                )
                .await?;

            // Create admin account (this will be our DAO account)
            let admin_account = {
                let id = account_n(99);
                let (secret_key, public_key) = near_sandbox::random_key_pair();
                sandbox.create_account(id.clone())
                    .initial_balance(NearToken::from_near(100))
                    .public_key(public_key)
                    .send()
                    .await?;
                let signer = Signer::from_secret_key(secret_key.parse()?)?;
                TestAccount { id, signer }
            };

            // Grant DAO role to admin account
            locker_contract
                .call(
                    "acl_grant_role",
                    json!({
                        "role": "DAO",
                        "account_id": admin_account.id,
                    }),
                    NearToken::from_yoctonear(0),
                    &network,
                )
                .await?;

            // Create sender account
            let sender_account = {
                let id = account_n(1);
                let (secret_key, public_key) = near_sandbox::random_key_pair();
                sandbox.create_account(id.clone())
                    .initial_balance(NearToken::from_near(100))
                    .public_key(public_key)
                    .send()
                    .await?;
                let signer = Signer::from_secret_key(secret_key.parse()?)?;
                TestAccount { id, signer }
            };

            // Register the accounts in the token contract
            token_contract
                .call(
                    "storage_deposit",
                    json!({
                        "account_id": locker_contract.id,
                        "registration_only": true,
                    }),
                    NEP141_DEPOSIT,
                    &network,
                )
                .await?;

            token_contract
                .call(
                    "storage_deposit",
                    json!({
                        "account_id": sender_account.id,
                        "registration_only": true,
                    }),
                    NEP141_DEPOSIT,
                    &network,
                )
                .await?;

            // Transfer initial tokens to the sender account
            token_contract
                .call(
                    "ft_transfer",
                    json!({
                        "receiver_id": sender_account.id,
                        "amount": U128(1_000_000),
                        "memo": None::<String>,
                    }),
                    NearToken::from_yoctonear(1),
                    &network,
                )
                .await?;

            // Add the ETH factory address to the locker contract
            let eth_factory_address = eth_factory_address();
            locker_contract
                .call(
                    "add_factory",
                    json!({
                        "address": eth_factory_address,
                    }),
                    NearToken::from_yoctonear(0),
                    &network,
                )
                .await?;

            Ok(Self {
                sandbox,
                network,
                token_contract,
                locker_contract,
                sender_account,
            })
        }

        async fn grant_native_fee_restricted_role(
            &self,
            account_id: &AccountId,
        ) -> anyhow::Result<()> {
            self.locker_contract
                .call(
                    "acl_grant_role",
                    json!({
                        "role": "NativeFeeRestricted",
                        "account_id": account_id,
                    }),
                    NearToken::from_yoctonear(0),
                    &self.network,
                )
                .await?;

            Ok(())
        }

        async fn revoke_native_fee_restricted_role(
            &self,
            account_id: &AccountId,
        ) -> anyhow::Result<()> {
            self.locker_contract
                .call(
                    "acl_revoke_role",
                    json!({
                        "role": "NativeFeeRestricted",
                        "account_id": account_id,
                    }),
                    NearToken::from_yoctonear(0),
                    &self.network,
                )
                .await?;

            Ok(())
        }

        async fn initialize_transfer(
            &self,
            amount: u128,
            native_fee: u128,
            token_fee: u128,
            should_succeed: bool,
        ) -> anyhow::Result<Option<TransferMessage>> {
            // Prepare storage deposit for the sender
            let required_balance_account: NearToken = self.locker_contract
                .view_no_args("required_balance_for_account", &self.network)
                .await?;

            let init_transfer_msg = InitTransferMsg {
                native_token_fee: U128(native_fee),
                fee: U128(token_fee),
                recipient: eth_eoa_address(),
                msg: None,
            };

            let required_balance_init_transfer: NearToken = self.locker_contract
                .view(
                    "required_balance_for_init_transfer",
                    json!({
                        "recipient": init_transfer_msg.recipient,
                        "sender": OmniAddress::Near(self.sender_account.id.clone()),
                    }),
                    &self.network,
                )
                .await?;

            // Deposit to storage
            let storage_deposit_amount = required_balance_account
                .saturating_add(NearToken::from_yoctonear(native_fee))
                .saturating_add(required_balance_init_transfer);

            self.sender_account
                .call(
                    &self.locker_contract.id,
                    "storage_deposit",
                    json!({
                        "account_id": self.sender_account.id,
                    }),
                    storage_deposit_amount,
                    &self.network,
                )
                .await?;

            // Initiate the transfer
            let transfer_result = self.sender_account
                .call(
                    &self.token_contract.id,
                    "ft_transfer_call",
                    json!({
                        "receiver_id": self.locker_contract.id,
                        "amount": U128(amount),
                        "memo": None::<String>,
                        "msg": serde_json::to_string(&init_transfer_msg)?,
                    }),
                    NearToken::from_yoctonear(1),
                    &self.network,
                )
                .await?;

            // Extract logs before consuming the result with .json()
            let logs: Vec<String> = transfer_result
                .logs()
                .iter()
                .map(|s| s.to_string())
                .collect();

            // For the case where we expect failure
            if !should_succeed {
                let returned: U128 = transfer_result.json()?;
                assert_eq!(U128(0), returned);
                return Ok(None);
            }
            let returned: U128 = transfer_result.json()?;
            assert_eq!(U128(amount), returned);

            let log_refs = logs.iter().collect::<Vec<&String>>();

            let omni_bridge_event: OmniBridgeEvent = serde_json::from_value(
                get_event_data("InitTransferEvent", &log_refs)?
                    .ok_or_else(|| anyhow::anyhow!("InitTransferEvent not found"))?,
            )?;

            let OmniBridgeEvent::InitTransferEvent { transfer_message } = omni_bridge_event else {
                anyhow::bail!("InitTransferEvent is found in unexpected event")
            };

            Ok(Some(transfer_message))
        }

        async fn create_account(&self, id: AccountId) -> anyhow::Result<TestAccount> {
            let (secret_key, public_key) = near_sandbox::random_key_pair();
            self.sandbox.create_account(id.clone())
                .initial_balance(NearToken::from_near(100))
                .public_key(public_key)
                .send()
                .await?;
            let signer = Signer::from_secret_key(secret_key.parse()?)?;
            Ok(TestAccount { id, signer })
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_native_fee_restriction(
        mock_token_wasm: Vec<u8>,
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(mock_token_wasm, mock_prover_wasm, locker_wasm).await?;

        // 1. Test that an account can set a native fee when not restricted
        let transfer_amount = 100;
        let native_fee = NearToken::from_near(1).as_yoctonear();
        let token_fee = 10;

        let transfer_message = env
            .initialize_transfer(
                transfer_amount,
                native_fee,
                token_fee,
                true, // Should succeed
            )
            .await?
            .unwrap();

        assert_eq!(
            transfer_message.fee.native_fee.0, native_fee,
            "Native fee was not set correctly"
        );

        // 2. Grant NativeFeeRestricted role to the sender account
        env.grant_native_fee_restricted_role(&env.sender_account.id)
            .await?;

        // 3. Test that the account cannot set a native fee when restricted
        let result = env
            .initialize_transfer(
                transfer_amount,
                native_fee,
                token_fee,
                false, // Should fail
            )
            .await;

        assert!(
            result.is_ok(),
            "Transfer should have failed with the expected error"
        );

        // 4. Test that the account can still transfer with zero native fee
        let transfer_message = env
            .initialize_transfer(
                transfer_amount,
                0, // Zero native fee
                token_fee,
                true, // Should succeed
            )
            .await?
            .unwrap();

        assert_eq!(
            transfer_message.fee.native_fee.0, 0,
            "Native fee should be zero"
        );

        // 5. Revoke the NativeFeeRestricted role
        env.revoke_native_fee_restricted_role(&env.sender_account.id)
            .await?;

        // 6. Test that the account can set a native fee after role revocation
        let transfer_message = env
            .initialize_transfer(
                transfer_amount,
                native_fee,
                token_fee,
                true, // Should succeed
            )
            .await?
            .unwrap();

        assert_eq!(
            transfer_message.fee.native_fee.0, native_fee,
            "Native fee was not set correctly after role revocation"
        );

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_role_persistence(
        mock_token_wasm: Vec<u8>,
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(mock_token_wasm, mock_prover_wasm, locker_wasm).await?;

        // 1. Check role is not granted initially
        let has_role: bool = env.locker_contract
            .view(
                "acl_has_role",
                json!({
                    "role": "NativeFeeRestricted",
                    "account_id": env.sender_account.id
                }),
                &env.network,
            )
            .await?;

        assert!(
            !has_role,
            "Account should not have NativeFeeRestricted role initially"
        );

        // 2. Grant the role
        env.grant_native_fee_restricted_role(&env.sender_account.id)
            .await?;

        // 3. Verify role is granted
        let has_role: bool = env.locker_contract
            .view(
                "acl_has_role",
                json!({
                    "role": "NativeFeeRestricted",
                    "account_id": env.sender_account.id
                }),
                &env.network,
            )
            .await?;

        assert!(
            has_role,
            "Account should have NativeFeeRestricted role after granting"
        );

        // 4. Revoke the role
        env.revoke_native_fee_restricted_role(&env.sender_account.id)
            .await?;

        // 5. Verify role is revoked
        let has_role: bool = env.locker_contract
            .view(
                "acl_has_role",
                json!({
                    "role": "NativeFeeRestricted",
                    "account_id": env.sender_account.id
                }),
                &env.network,
            )
            .await?;

        assert!(
            !has_role,
            "Account should not have NativeFeeRestricted role after revoking"
        );

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_admin_permissions(
        mock_token_wasm: Vec<u8>,
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(mock_token_wasm, mock_prover_wasm, locker_wasm).await?;

        // Create a new account without special permissions
        let unauthorized_account = env.create_account(account_n(42)).await?;

        // Try to grant NativeFeeRestricted role using unauthorized account
        let _result = unauthorized_account
            .call(
                &env.locker_contract.id,
                "acl_grant_role",
                json!({
                    "role": "NativeFeeRestricted",
                    "account_id": env.sender_account.id,
                }),
                NearToken::from_yoctonear(0),
                &env.network,
            )
            .await;

        // Verify that the role was NOT granted, regardless of whether the call succeeded or failed
        let role_granted: bool = env.locker_contract
            .view(
                "acl_has_role",
                json!({
                    "role": "NativeFeeRestricted",
                    "account_id": env.sender_account.id
                }),
                &env.network,
            )
            .await?;

        assert!(
            !role_granted,
            "Role should not be granted by unauthorized account"
        );

        // Verify that authorized admin can grant the role
        env.grant_native_fee_restricted_role(&env.sender_account.id)
            .await?;

        // Verify role was successfully granted
        let has_role: bool = env.locker_contract
            .view(
                "acl_has_role",
                json!({
                    "role": "NativeFeeRestricted",
                    "account_id": env.sender_account.id
                }),
                &env.network,
            )
            .await?;

        assert!(has_role, "DAO account should be able to grant roles");

        Ok(())
    }
}
