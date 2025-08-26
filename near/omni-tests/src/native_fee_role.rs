#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
    };
    use near_workspaces::{types::NearToken, AccountId};
    use omni_types::{near_events::OmniBridgeEvent, InitTransferMsg, OmniAddress, TransferMessage};
    use rstest::rstest;

    use crate::helpers::tests::{
        account_n, eth_eoa_address, eth_factory_address, get_event_data, locker_wasm,
        mock_prover_wasm, mock_token_wasm, NEP141_DEPOSIT,
    };

    struct TestEnv {
        worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
        token_contract: near_workspaces::Contract,
        locker_contract: near_workspaces::Contract,
        sender_account: near_workspaces::Account,
    }

    impl TestEnv {
        #[allow(clippy::too_many_lines)]
        async fn new(
            mock_token_wasm: Vec<u8>,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            let worker = near_workspaces::sandbox().await?;

            // Deploy and initialize FT token
            let token_contract = worker.dev_deploy(&mock_token_wasm).await?;
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

            // Deploy and initialize locker
            let locker_contract = worker.dev_deploy(&locker_wasm).await?;
            locker_contract
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
            locker_contract
                .call("add_prover")
                .args_json(json!({
                    "prover_id": "Eth",
                    "account_id": prover.id(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Create admin account (this will be our DAO account)
            let admin_account = worker
                .create_tla(account_n(99), worker.dev_generate().await.1)
                .await?
                .unwrap();

            // Grant DAO role to admin account
            locker_contract
                .call("acl_grant_role")
                .args_json(json!({
                    "role": "DAO",
                    "account_id": admin_account.id(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Create sender account
            let sender_account = worker
                .create_tla(account_n(1), worker.dev_generate().await.1)
                .await?
                .unwrap();

            // Register the accounts in the token contract
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
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": sender_account.id(),
                    "registration_only": true,
                }))
                .deposit(NEP141_DEPOSIT)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Transfer initial tokens to the sender account
            token_contract
                .call("ft_transfer")
                .args_json(json!({
                    "receiver_id": sender_account.id(),
                    "amount": U128(1_000_000),
                    "memo": None::<String>,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Add the ETH factory address to the locker contract
            let eth_factory_address = eth_factory_address();
            locker_contract
                .call("add_factory")
                .args_json(json!({
                    "address": eth_factory_address,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(Self {
                worker,
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
                .call("acl_grant_role")
                .args_json(json!({
                    "role": "NativeFeeRestricted",
                    "account_id": account_id,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(())
        }

        async fn revoke_native_fee_restricted_role(
            &self,
            account_id: &AccountId,
        ) -> anyhow::Result<()> {
            self.locker_contract
                .call("acl_revoke_role")
                .args_json(json!({
                    "role": "NativeFeeRestricted",
                    "account_id": account_id,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

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
            let required_balance_account: NearToken = self
                .locker_contract
                .view("required_balance_for_account")
                .await?
                .json()?;

            let init_transfer_msg = InitTransferMsg {
                native_token_fee: U128(native_fee),
                fee: U128(token_fee),
                recipient: eth_eoa_address(),
            };

            let required_balance_init_transfer: NearToken = self
                .locker_contract
                .view("required_balance_for_init_transfer")
                .args_json(json!({
                    "recipient": init_transfer_msg.recipient,
                    "sender": OmniAddress::Near(self.sender_account.id().clone()),
                }))
                .await?
                .json()?;

            // Deposit to storage
            let storage_deposit_amount = required_balance_account
                .saturating_add(NearToken::from_yoctonear(native_fee))
                .saturating_add(required_balance_init_transfer);

            self.sender_account
                .call(self.locker_contract.id(), "storage_deposit")
                .args_json(json!({
                    "account_id": self.sender_account.id(),
                }))
                .deposit(storage_deposit_amount)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Initiate the transfer
            let transfer_result = self
                .sender_account
                .call(self.token_contract.id(), "ft_transfer_call")
                .args_json(json!({
                    "receiver_id": self.locker_contract.id(),
                    "amount": U128(amount),
                    "memo": None::<String>,
                    "msg": serde_json::to_string(&init_transfer_msg)?,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?;

            // For the case where we expect failure
            if !should_succeed {
                // Check if any of the receipt outcomes contain our expected error message
                let contains_expected_error =
                    transfer_result.receipt_outcomes().iter().any(|outcome| {
                        // Convert outcome to string to check for the error message
                        let outcome_str = format!("{outcome:?}");
                        outcome_str.contains("ERR_ACCOUNT_RESTRICTED_FROM_USING_NATIVE_FEE")
                    });

                assert!(contains_expected_error,
                    "Expected to find ERR_ACCOUNT_RESTRICTED_FROM_USING_NATIVE_FEE error in receipts");
                return Ok(None);
            }

            // For successful case, extract the transfer message
            let logs = transfer_result
                .logs()
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<String>>();

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
        env.grant_native_fee_restricted_role(env.sender_account.id())
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
        env.revoke_native_fee_restricted_role(env.sender_account.id())
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
        let has_role: bool = env
            .locker_contract
            .view("acl_has_role")
            .args_json(json!({
                "role": "NativeFeeRestricted",
                "account_id": env.sender_account.id()
            }))
            .await?
            .json()?;

        assert!(
            !has_role,
            "Account should not have NativeFeeRestricted role initially"
        );

        // 2. Grant the role
        env.grant_native_fee_restricted_role(env.sender_account.id())
            .await?;

        // 3. Verify role is granted
        let has_role: bool = env
            .locker_contract
            .view("acl_has_role")
            .args_json(json!({
                "role": "NativeFeeRestricted",
                "account_id": env.sender_account.id()
            }))
            .await?
            .json()?;

        assert!(
            has_role,
            "Account should have NativeFeeRestricted role after granting"
        );

        // 4. Revoke the role
        env.revoke_native_fee_restricted_role(env.sender_account.id())
            .await?;

        // 5. Verify role is revoked
        let has_role: bool = env
            .locker_contract
            .view("acl_has_role")
            .args_json(json!({
                "role": "NativeFeeRestricted",
                "account_id": env.sender_account.id()
            }))
            .await?
            .json()?;

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
        let unauthorized_account = env
            .worker
            .create_tla(account_n(42), env.worker.dev_generate().await.1)
            .await?
            .unwrap();

        // Try to grant NativeFeeRestricted role using unauthorized account
        let _result = unauthorized_account
            .call(env.locker_contract.id(), "acl_grant_role")
            .args_json(json!({
                "role": "NativeFeeRestricted",
                "account_id": env.sender_account.id(),
            }))
            .max_gas()
            .transact()
            .await;

        // Verify that the role was NOT granted, regardless of whether the call succeeded or failed
        let role_granted: bool = env
            .locker_contract
            .view("acl_has_role")
            .args_json(json!({
                "role": "NativeFeeRestricted",
                "account_id": env.sender_account.id()
            }))
            .await?
            .json()?;

        assert!(
            !role_granted,
            "Role should not be granted by unauthorized account"
        );

        // Verify that authorized admin can grant the role
        env.grant_native_fee_restricted_role(env.sender_account.id())
            .await?;

        // Verify role was successfully granted
        let has_role: bool = env
            .locker_contract
            .view("acl_has_role")
            .args_json(json!({
                "role": "NativeFeeRestricted",
                "account_id": env.sender_account.id()
            }))
            .await?
            .json()?;

        assert!(has_role, "DAO account should be able to grant roles");

        Ok(())
    }
}
