#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use near_sdk::borsh;
    use near_sdk::json_types::U128;
    use near_sdk::serde_json::json;
    use near_workspaces::{types::NearToken, AccountId};
    use omni_types::locker_args::{FinTransferArgs, StorageDepositAction};
    use omni_types::prover_result::InitTransferMessage;
    use omni_types::prover_result::ProverResult;
    use omni_types::Fee;
    use omni_types::{BasicMetadata, ChainKind, OmniAddress};
    use rstest::rstest;

    use crate::helpers::tests::{
        account_n, arb_factory_address, arb_token_address, base_factory_address,
        base_token_address, bnb_factory_address, bnb_token_address, eth_eoa_address,
        eth_factory_address, eth_token_address, get_test_deploy_token_args, locker_wasm,
        mock_prover_wasm, sol_factory_address, sol_token_address, token_deployer_wasm,
        NEP141_DEPOSIT,
    };

    struct TestEnv {
        worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
        locker_contract: near_workspaces::Contract,
        token_contract: near_workspaces::Contract,
        init_token_address: OmniAddress,
        factory_contract_address: OmniAddress,
        token_metadata: BasicMetadata,
    }

    impl TestEnv {
        async fn new(
            init_token_address: OmniAddress,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
            token_deployer_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            let worker = near_workspaces::sandbox().await?;
            let token_metadata = BasicMetadata {
                name: "Test Token".to_string(),
                symbol: "TEST".to_string(),
                decimals: 18,
            };

            // setup locker
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

            for chain in ["Eth", "Base", "Arb", "Bnb", "Sol"] {
                locker_contract
                    .call("add_prover")
                    .args_json(json!({ "chain": chain, "account_id": prover.id() }))
                    .max_gas()
                    .transact()
                    .await?
                    .into_result()?;
            }

            // Setup token deployer
            let token_deployer = worker
                .create_tla_and_deploy(
                    account_n(1),
                    worker.dev_generate().await.1,
                    &token_deployer_wasm,
                )
                .await?
                .unwrap();

            token_deployer
                .call("new")
                .args_json(json!({
                    "controller": locker_contract.id(),
                    "dao": AccountId::from_str("dao.near").unwrap(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Configure locker
            locker_contract
                .call("add_token_deployer")
                .args_json(json!({
                    "chain": init_token_address.get_chain(),
                    "account_id": token_deployer.id(),
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let factory_contract_address = match init_token_address.get_chain() {
                ChainKind::Eth => eth_factory_address(),
                ChainKind::Sol => sol_factory_address(),
                ChainKind::Arb => arb_factory_address(),
                ChainKind::Base => base_factory_address(),
                ChainKind::Bnb => bnb_factory_address(),
                ChainKind::Near => panic!("Unsupported chain"),
            };

            locker_contract
                .call("add_factory")
                .args_json(json!({
                    "address": factory_contract_address,
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            // Deploy token
            let token_contract = Self::deploy_token(
                &worker,
                &locker_contract,
                &init_token_address,
                &factory_contract_address,
                &token_metadata,
            )
            .await?;

            Ok(Self {
                worker,
                locker_contract,
                token_contract,
                init_token_address,
                factory_contract_address,
                token_metadata,
            })
        }

        async fn new_native(
            chain_kind: ChainKind,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
            token_deployer_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            let init_token_address = OmniAddress::new_zero(chain_kind).unwrap();
            Self::new(
                init_token_address,
                mock_prover_wasm,
                locker_wasm,
                token_deployer_wasm,
            )
            .await
        }

        async fn deploy_token(
            worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
            locker: &near_workspaces::Contract,
            init_token_address: &OmniAddress,
            factoty_contract_address: &OmniAddress,
            token_metadata: &BasicMetadata,
        ) -> anyhow::Result<near_workspaces::Contract> {
            let token_deploy_initiator = worker
                .create_tla(account_n(2), worker.dev_generate().await.1)
                .await?
                .unwrap();

            let required_storage: NearToken = locker
                .view("required_balance_for_deploy_token")
                .await?
                .json()?;

            if init_token_address == &OmniAddress::new_zero(init_token_address.get_chain()).unwrap()
            {
                locker
                    .call("deploy_native_token")
                    .args_json(json!({
                        "chain_kind": init_token_address.get_chain(),
                        "name": token_metadata.name,
                        "symbol": token_metadata.symbol,
                        "decimals": token_metadata.decimals,
                    }))
                    .deposit(required_storage)
                    .max_gas()
                    .transact()
                    .await?
                    .into_result()?;
            } else {
                token_deploy_initiator
                    .call(locker.id(), "deploy_token")
                    .args_borsh(get_test_deploy_token_args(
                        init_token_address,
                        factoty_contract_address,
                        token_metadata,
                    ))
                    .deposit(required_storage)
                    .max_gas()
                    .transact()
                    .await?
                    .into_result()?;
            }

            let token_account_id: AccountId = locker
                .view("get_token_id")
                .args_json(json!({
                    "address": init_token_address
                }))
                .await?
                .json()?;

            let token_contract = worker
                .import_contract(&token_account_id, worker)
                .transact()
                .await?;

            Ok(token_contract)
        }

        // Helper to create and register a new account
        async fn create_registered_account(
            &self,
            account_num: u8,
        ) -> anyhow::Result<near_workspaces::Account> {
            let account = self
                .worker
                .create_tla(account_n(account_num), self.worker.dev_generate().await.1)
                .await?
                .unwrap();

            self.token_contract
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": Some(account.id()),
                    "registration_only": Some(true),
                }))
                .deposit(NEP141_DEPOSIT)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            Ok(account)
        }
    }

    #[rstest]
    #[case(eth_token_address(), false)]
    #[case(sol_token_address(), false)]
    #[case(arb_token_address(), false)]
    #[case(base_token_address(), false)]
    #[case(bnb_token_address(), false)]
    #[case(eth_token_address(), true)]
    #[case(sol_token_address(), true)]
    #[case(arb_token_address(), true)]
    #[case(base_token_address(), true)]
    #[case(bnb_token_address(), true)]
    #[tokio::test]
    async fn test_token_metadata(
        #[case] init_token_address: OmniAddress,
        #[case] is_native: bool,
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = if is_native {
            TestEnv::new_native(
                init_token_address.get_chain(),
                mock_prover_wasm,
                locker_wasm,
                token_deployer_wasm,
            )
            .await?
        } else {
            TestEnv::new(
                init_token_address,
                mock_prover_wasm,
                locker_wasm,
                token_deployer_wasm,
            )
            .await?
        };

        let fetched_metadata: BasicMetadata =
            env.token_contract.view("ft_metadata").await?.json()?;

        assert_eq!(env.token_metadata.name, fetched_metadata.name);
        assert_eq!(env.token_metadata.symbol, fetched_metadata.symbol);
        assert_eq!(env.token_metadata.decimals, fetched_metadata.decimals);

        Ok(())
    }

    #[rstest]
    #[case(eth_token_address())]
    #[case(sol_token_address())]
    #[case(arb_token_address())]
    #[case(base_token_address())]
    #[case(bnb_token_address())]
    #[tokio::test]
    async fn test_set_token_metadata(
        #[case] init_token_address: OmniAddress,
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(
            init_token_address,
            mock_prover_wasm,
            locker_wasm,
            token_deployer_wasm,
        )
        .await?;

        let fetched_metadata: BasicMetadata =
            env.token_contract.view("ft_metadata").await?.json()?;

        assert_eq!(env.token_metadata.name, fetched_metadata.name);
        assert_eq!(env.token_metadata.symbol, fetched_metadata.symbol);
        assert_eq!(env.token_metadata.decimals, fetched_metadata.decimals);

        env.locker_contract
            .call("set_token_metadata")
            .args_json(json!({
                "address": env.init_token_address,
                "name": "New Token Name",
                "symbol": "NEW"
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let updated_metadata: BasicMetadata =
            env.token_contract.view("ft_metadata").await?.json()?;

        assert_eq!(updated_metadata.name, "New Token Name");
        assert_eq!(updated_metadata.symbol, "NEW");
        assert_eq!(updated_metadata.decimals, fetched_metadata.decimals);

        Ok(())
    }

    #[rstest]
    #[case(eth_token_address(), false)]
    #[case(sol_token_address(), false)]
    #[case(arb_token_address(), false)]
    #[case(base_token_address(), false)]
    #[case(bnb_token_address(), false)]
    #[case(eth_token_address(), true)]
    #[case(sol_token_address(), true)]
    #[case(arb_token_address(), true)]
    #[case(base_token_address(), true)]
    #[case(bnb_token_address(), true)]
    #[tokio::test]
    async fn test_token_minting(
        #[case] init_token_address: OmniAddress,
        #[case] is_native: bool,
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = if is_native {
            TestEnv::new_native(
                init_token_address.get_chain(),
                mock_prover_wasm,
                locker_wasm,
                token_deployer_wasm,
            )
            .await?
        } else {
            TestEnv::new(
                init_token_address,
                mock_prover_wasm,
                locker_wasm,
                token_deployer_wasm,
            )
            .await?
        };
        let recipient = env.create_registered_account(3).await?;
        let amount = U128(1_000_000_000_000_000_000_000_000);

        fake_finalize_transfer(
            &env.locker_contract,
            &env.token_contract,
            &recipient,
            env.init_token_address,
            env.factory_contract_address,
            amount,
        )
        .await?;

        let balance: U128 = env
            .token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": recipient.id(),
            }))
            .await?
            .json()?;

        let total_supply: U128 = env.token_contract.view("ft_total_supply").await?.json()?;

        assert_eq!(
            balance, amount,
            "Balance should be equal to the minted amount"
        );
        assert_eq!(
            total_supply, amount,
            "Total supply should be equal to the minted amount"
        );
        Ok(())
    }

    #[rstest]
    #[case(eth_token_address(), false)]
    #[case(sol_token_address(), false)]
    #[case(arb_token_address(), false)]
    #[case(base_token_address(), false)]
    #[case(bnb_token_address(), false)]
    #[case(eth_token_address(), true)]
    #[case(sol_token_address(), true)]
    #[case(arb_token_address(), true)]
    #[case(base_token_address(), true)]
    #[case(bnb_token_address(), true)]
    #[tokio::test]
    async fn test_token_transfer(
        #[case] init_token_address: OmniAddress,
        #[case] is_native: bool,
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = if is_native {
            TestEnv::new_native(
                init_token_address.get_chain(),
                mock_prover_wasm,
                locker_wasm,
                token_deployer_wasm,
            )
            .await?
        } else {
            TestEnv::new(
                init_token_address,
                mock_prover_wasm,
                locker_wasm,
                token_deployer_wasm,
            )
            .await?
        };
        let sender = env.create_registered_account(3).await?;
        let receiver = env.create_registered_account(4).await?;
        let amount = U128(1_000_000_000_000_000_000_000_000);

        // Mint tokens to sender
        fake_finalize_transfer(
            &env.locker_contract,
            &env.token_contract,
            &sender,
            env.init_token_address,
            env.factory_contract_address,
            amount,
        )
        .await?;

        // Transfer tokens
        sender
            .call(env.token_contract.id(), "ft_transfer")
            .args_json(json!({
                "receiver_id": receiver.id(),
                "amount": amount,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Verify balances
        let sender_balance: U128 = env
            .token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": sender.id(),
            }))
            .await?
            .json()?;

        let receiver_balance: U128 = env
            .token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": receiver.id(),
            }))
            .await?
            .json()?;

        let total_supply: U128 = env.token_contract.view("ft_total_supply").await?.json()?;

        assert_eq!(sender_balance, U128(0), "Sender balance should be 0");
        assert_eq!(
            receiver_balance, amount,
            "Receiver balance should be equal to the sent amount"
        );
        assert_eq!(
            total_supply, amount,
            "Total supply should be equal to the minted amount"
        );

        Ok(())
    }

    async fn fake_finalize_transfer(
        locker_contract: &near_workspaces::Contract,
        token_contract: &near_workspaces::Contract,
        recipient: &near_workspaces::Account,
        token_address: OmniAddress,
        emitter_address: OmniAddress,
        amount: U128,
    ) -> anyhow::Result<()> {
        let storage_deposit_actions = vec![StorageDepositAction {
            token_id: token_contract.id().clone(),
            account_id: recipient.id().clone(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        }];
        let required_balance_for_fin_transfer: NearToken = locker_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;
        let required_deposit_for_fin_transfer =
            NEP141_DEPOSIT.saturating_add(required_balance_for_fin_transfer);

        // Simulate finalization of transfer through locker
        locker_contract
            .call("fin_transfer")
            .args_borsh(FinTransferArgs {
                chain_kind: ChainKind::Eth,
                storage_deposit_actions,
                prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                    origin_nonce: 1,
                    token: token_address,
                    recipient: OmniAddress::Near(recipient.id().clone()),
                    amount,
                    fee: Fee {
                        fee: U128(0),
                        native_fee: U128(0),
                    },
                    sender: eth_eoa_address(),
                    msg: String::default(),
                    emitter_address,
                }))?,
            })
            .deposit(required_deposit_for_fin_transfer)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(())
    }
}
