#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use near_sdk::json_types::{Base64VecU8, U128};
    use near_sdk::serde_json::json;
    use near_sdk::{borsh, json_types::Base58CryptoHash, CryptoHash};
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
        mock_global_contract_deployer_wasm, mock_prover_wasm, omni_token_wasm, pol_factory_address,
        sol_factory_address, sol_token_address, token_deployer_wasm, wasm_code_hash,
        GLOBAL_STORAGE_COST_PER_BYTE, NEP141_DEPOSIT, STORAGE_DEPOSIT_PER_BYTE,
    };

    const PREV_TOKEN_DEPLOYER_WASM_FILEPATH: &str = "src/data/legacy_token_deployer-0.2.4.wasm";

    #[derive(Clone, Copy)]
    enum DepositStrategy {
        MinimumRequired,
        WithBuffer,
    }

    struct TestEnv {
        worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
        locker_contract: near_workspaces::Contract,
        token_account_id: AccountId,
        init_token_address: OmniAddress,
        factory_contract_address: OmniAddress,
        token_metadata: BasicMetadata,
    }

    struct DeployEnv {
        worker: near_workspaces::Worker<near_workspaces::network::Sandbox>,
        locker_contract: near_workspaces::Contract,
        init_token_address: OmniAddress,
        factory_contract_address: OmniAddress,
        token_metadata: BasicMetadata,
        omni_token_wasm_len: usize,
    }

    impl DeployEnv {
        #[allow(clippy::too_many_arguments)]
        async fn new(
            init_token_address: OmniAddress,
            token_metadata: BasicMetadata,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
            omni_token_wasm: Vec<u8>,
            token_deployer_wasm: Vec<u8>,
            mock_global_contract_deployer_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            let worker = near_workspaces::sandbox().await?;

            // setup locker
            let locker_contract = worker.dev_deploy(&locker_wasm).await?;
            locker_contract
                .call("new")
                .args_json(json!({
                    "mpc_signer": "mpc.testnet",
                    "nonce": U128(0),
                    "wnear_account_id": "wnear.testnet",
                    "btc_connector": "brg-dev.testnet",
                }))
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            let prover = worker.dev_deploy(&mock_prover_wasm).await?;

            for chain in ["Eth", "Base", "Arb", "Bnb", "Pol", "Sol"] {
                locker_contract
                    .call("add_prover")
                    .args_json(json!({ "chain": chain, "account_id": prover.id() }))
                    .max_gas()
                    .transact()
                    .await?
                    .into_result()?;
            }

            // Deploy global omni token contract
            let omni_token_code_hash = TestEnv::deploy_global_omni_token(
                &worker,
                &omni_token_wasm,
                &mock_global_contract_deployer_wasm,
            )
            .await?;
            let global_code_hash = Base58CryptoHash::from(omni_token_code_hash);

            // Setup token deployer
            let token_deployer = worker
                .create_tla_and_deploy(
                    account_n(1),
                    worker.generate_dev_account_credentials().1,
                    &token_deployer_wasm,
                )
                .await?
                .unwrap();

            token_deployer
                .call("new")
                .args_json(json!({
                    "controller": locker_contract.id(),
                    "dao": AccountId::from_str("dao.near").unwrap(),
                    "global_code_hash": global_code_hash,
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
                ChainKind::Pol => pol_factory_address(),
                ChainKind::Near | ChainKind::Btc | ChainKind::Zcash => panic!("Unsupported chain"),
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

            Ok(Self {
                worker,
                locker_contract,
                init_token_address,
                factory_contract_address,
                token_metadata,
                omni_token_wasm_len: omni_token_wasm.len(),
            })
        }

        async fn deploy_token(
            &self,
            deposit_strategy: DepositStrategy,
            deploy_initiator_index: u8,
        ) -> anyhow::Result<AccountId> {
            TestEnv::deploy_token(
                &self.worker,
                &self.locker_contract,
                &self.init_token_address,
                &self.factory_contract_address,
                &self.token_metadata,
                self.omni_token_wasm_len,
                deposit_strategy,
                deploy_initiator_index,
            )
            .await
        }

        fn into_test_env(self, token_account_id: AccountId) -> TestEnv {
            TestEnv {
                worker: self.worker,
                locker_contract: self.locker_contract,
                token_account_id,
                init_token_address: self.init_token_address,
                factory_contract_address: self.factory_contract_address,
                token_metadata: self.token_metadata,
            }
        }
    }

    impl TestEnv {
        fn default_token_metadata() -> BasicMetadata {
            BasicMetadata {
                name: "Test Token".to_string(),
                symbol: "TEST".to_string(),
                decimals: 18,
            }
        }

        async fn new(
            init_token_address: OmniAddress,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
            omni_token_wasm: Vec<u8>,
            token_deployer_wasm: Vec<u8>,
            mock_global_contract_deployer_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            Self::new_with_metadata(
                init_token_address,
                Self::default_token_metadata(),
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await
        }

        fn deposit_with_metadata_buffer(
            required_storage: NearToken,
            token_metadata: &BasicMetadata,
            omni_token_wasm_len: usize,
        ) -> NearToken {
            let base_metadata = Self::default_token_metadata();
            let base_len = base_metadata
                .name
                .len()
                .saturating_add(base_metadata.symbol.len());
            let metadata_len = token_metadata
                .name
                .len()
                .saturating_add(token_metadata.symbol.len());
            let metadata_delta = metadata_len.saturating_sub(base_len);

            let code_storage_deposit = STORAGE_DEPOSIT_PER_BYTE
                .saturating_mul(omni_token_wasm_len.try_into().unwrap_or_default());
            let metadata_storage_deposit = STORAGE_DEPOSIT_PER_BYTE
                .saturating_mul(metadata_delta.try_into().unwrap_or_default());

            required_storage
                .saturating_add(code_storage_deposit)
                .saturating_add(metadata_storage_deposit)
                .saturating_add(NearToken::from_near(5))
        }

        async fn new_native(
            chain_kind: ChainKind,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
            omni_token_wasm: Vec<u8>,
            token_deployer_wasm: Vec<u8>,
            mock_global_contract_deployer_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            let init_token_address = OmniAddress::new_zero(chain_kind).unwrap();
            Self::new_with_metadata_and_strategy(
                init_token_address,
                Self::default_token_metadata(),
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
                DepositStrategy::WithBuffer,
            )
            .await
        }

        async fn new_with_metadata(
            init_token_address: OmniAddress,
            token_metadata: BasicMetadata,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
            omni_token_wasm: Vec<u8>,
            token_deployer_wasm: Vec<u8>,
            mock_global_contract_deployer_wasm: Vec<u8>,
        ) -> anyhow::Result<Self> {
            Self::new_with_metadata_and_strategy(
                init_token_address,
                token_metadata,
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
                DepositStrategy::WithBuffer,
            )
            .await
        }

        #[allow(clippy::too_many_arguments)]
        async fn new_with_metadata_and_strategy(
            init_token_address: OmniAddress,
            token_metadata: BasicMetadata,
            mock_prover_wasm: Vec<u8>,
            locker_wasm: Vec<u8>,
            omni_token_wasm: Vec<u8>,
            token_deployer_wasm: Vec<u8>,
            mock_global_contract_deployer_wasm: Vec<u8>,
            deposit_strategy: DepositStrategy,
        ) -> anyhow::Result<Self> {
            let deploy_env = DeployEnv::new(
                init_token_address,
                token_metadata,
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await?;

            let token_account_id = deploy_env.deploy_token(deposit_strategy, 2).await?;

            Ok(deploy_env.into_test_env(token_account_id))
        }

        async fn deploy_global_omni_token(
            worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
            omni_token_wasm: &[u8],
            mock_global_contract_deployer_wasm: &[u8],
        ) -> anyhow::Result<CryptoHash> {
            let mock_global_contract_deployer = worker
                .dev_deploy(mock_global_contract_deployer_wasm)
                .await?;

            let omni_token_global_contract_id: AccountId =
                format!("omni-token-global.{}", mock_global_contract_deployer.id()).parse()?;
            let omni_token_code_hash = wasm_code_hash(omni_token_wasm);

            mock_global_contract_deployer
                .call("deploy_global_contract")
                .args_json((
                    Base64VecU8::from(omni_token_wasm.to_vec()),
                    &omni_token_global_contract_id,
                ))
                .max_gas()
                .deposit(
                    GLOBAL_STORAGE_COST_PER_BYTE
                        .saturating_mul(omni_token_wasm.len().try_into().unwrap()),
                )
                .transact()
                .await?
                .into_result()?;

            Ok(omni_token_code_hash)
        }

        #[allow(clippy::too_many_arguments)]
        async fn deploy_token(
            worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
            locker: &near_workspaces::Contract,
            init_token_address: &OmniAddress,
            factoty_contract_address: &OmniAddress,
            token_metadata: &BasicMetadata,
            omni_token_wasm_len: usize,
            deposit_strategy: DepositStrategy,
            deploy_initiator_index: u8,
        ) -> anyhow::Result<AccountId> {
            let token_deploy_initiator = worker
                .create_tla(
                    account_n(deploy_initiator_index),
                    worker.generate_dev_account_credentials().1,
                )
                .await?
                .unwrap();

            let required_storage: NearToken = locker
                .view("required_balance_for_deploy_token")
                .await?
                .json()?;
            let deploy_deposit = match deposit_strategy {
                DepositStrategy::WithBuffer => Self::deposit_with_metadata_buffer(
                    required_storage,
                    token_metadata,
                    omni_token_wasm_len,
                ),
                DepositStrategy::MinimumRequired => required_storage,
            };

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
                    .deposit(deploy_deposit)
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
                    .deposit(deploy_deposit)
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

            Ok(token_account_id)
        }

        // Helper to create and register a new account
        async fn create_registered_account(
            &self,
            account_num: u8,
        ) -> anyhow::Result<near_workspaces::Account> {
            let account = self
                .worker
                .create_tla(
                    account_n(account_num),
                    self.worker.generate_dev_account_credentials().1,
                )
                .await?
                .unwrap();

            account
                .call(&self.token_account_id, "storage_deposit")
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
        omni_token_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = if is_native {
            TestEnv::new_native(
                init_token_address.get_chain(),
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await?
        } else {
            TestEnv::new(
                init_token_address,
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await?
        };

        let fetched_metadata: BasicMetadata = env
            .worker
            .view(&env.token_account_id, "ft_metadata")
            .await?
            .json()?;

        assert_eq!(env.token_metadata.name, fetched_metadata.name);
        assert_eq!(env.token_metadata.symbol, fetched_metadata.symbol);
        assert_eq!(env.token_metadata.decimals, fetched_metadata.decimals);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_deploy_token_with_huge_metadata(
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
        omni_token_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let huge_name = "TEST NAME".repeat(256);
        let huge_symbol = "TEST".repeat(64);

        let env = TestEnv::new_with_metadata(
            eth_token_address(),
            BasicMetadata {
                name: huge_name.clone(),
                symbol: huge_symbol.clone(),
                decimals: 18,
            },
            mock_prover_wasm,
            locker_wasm,
            omni_token_wasm,
            token_deployer_wasm,
            mock_global_contract_deployer_wasm,
        )
        .await?;

        let fetched_metadata: BasicMetadata = env
            .worker
            .view(&env.token_account_id, "ft_metadata")
            .await?
            .json()?;

        assert_eq!(fetched_metadata.name, huge_name);
        assert_eq!(fetched_metadata.symbol, huge_symbol);
        assert_eq!(fetched_metadata.decimals, 18);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_deploy_token_with_huge_metadata_insufficient_deposit(
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
        omni_token_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let token_metadata = BasicMetadata {
            name: "TEST NAME".repeat(256),
            symbol: "TEST".repeat(64),
            decimals: 18,
        };

        let deploy_env = DeployEnv::new(
            eth_token_address(),
            token_metadata.clone(),
            mock_prover_wasm,
            locker_wasm,
            omni_token_wasm,
            token_deployer_wasm,
            mock_global_contract_deployer_wasm,
        )
        .await?;

        let err = match deploy_env
            .deploy_token(DepositStrategy::MinimumRequired, 2)
            .await
        {
            Ok(_) => panic!("deployment with minimal deposit should fail"),
            Err(err) => err,
        };

        let err_string = err.to_string();
        assert!(
            err_string.contains("LackBalance")
                || err_string.contains("AccountDoesNotExist")
                || err_string.contains("doesn't exist")
                || err_string.contains("unable to fulfill the query request"),
            "unexpected error for insufficient deposit: {err_string}"
        );

        let token_account_id = deploy_env
            .deploy_token(DepositStrategy::WithBuffer, 3)
            .await?;

        let env = deploy_env.into_test_env(token_account_id);

        let fetched_metadata: BasicMetadata = env
            .worker
            .view(&env.token_account_id, "ft_metadata")
            .await?
            .json()?;

        assert_eq!(fetched_metadata.name, token_metadata.name);
        assert_eq!(fetched_metadata.symbol, token_metadata.symbol);
        assert_eq!(fetched_metadata.decimals, token_metadata.decimals);

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
        omni_token_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(
            init_token_address,
            mock_prover_wasm,
            locker_wasm,
            omni_token_wasm,
            token_deployer_wasm,
            mock_global_contract_deployer_wasm,
        )
        .await?;

        let fetched_metadata: BasicMetadata = env
            .worker
            .view(&env.token_account_id, "ft_metadata")
            .await?
            .json()?;

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

        let updated_metadata: BasicMetadata = env
            .worker
            .view(&env.token_account_id, "ft_metadata")
            .await?
            .json()?;

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
        omni_token_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = if is_native {
            TestEnv::new_native(
                init_token_address.get_chain(),
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await?
        } else {
            TestEnv::new(
                init_token_address,
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await?
        };
        let recipient = env.create_registered_account(3).await?;
        let amount = U128(1_000_000_000_000_000_000_000_000);

        fake_finalize_transfer(
            &env.locker_contract,
            &env.token_account_id,
            &recipient,
            env.init_token_address,
            env.factory_contract_address,
            amount,
        )
        .await?;

        let balance: U128 = env
            .worker
            .view(&env.token_account_id, "ft_balance_of")
            .args_json(json!({
                "account_id": recipient.id(),
            }))
            .await?
            .json()?;

        let total_supply: U128 = env
            .worker
            .view(&env.token_account_id, "ft_total_supply")
            .await?
            .json()?;

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
        omni_token_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = if is_native {
            TestEnv::new_native(
                init_token_address.get_chain(),
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await?
        } else {
            TestEnv::new(
                init_token_address,
                mock_prover_wasm,
                locker_wasm,
                omni_token_wasm,
                token_deployer_wasm,
                mock_global_contract_deployer_wasm,
            )
            .await?
        };
        let sender = env.create_registered_account(3).await?;
        let receiver = env.create_registered_account(4).await?;
        let amount = U128(1_000_000_000_000_000_000_000_000);

        // Mint tokens to sender
        fake_finalize_transfer(
            &env.locker_contract,
            &env.token_account_id,
            &sender,
            env.init_token_address,
            env.factory_contract_address,
            amount,
        )
        .await?;

        // Transfer tokens
        sender
            .call(&env.token_account_id, "ft_transfer")
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
            .worker
            .view(&env.token_account_id, "ft_balance_of")
            .args_json(json!({
                "account_id": sender.id(),
            }))
            .await?
            .json()?;

        let receiver_balance: U128 = env
            .worker
            .view(&env.token_account_id, "ft_balance_of")
            .args_json(json!({
                "account_id": receiver.id(),
            }))
            .await?
            .json()?;

        let total_supply: U128 = env
            .worker
            .view(&env.token_account_id, "ft_total_supply")
            .await?
            .json()?;

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

    #[rstest]
    #[tokio::test]
    async fn test_token_deployer_migration(
        token_deployer_wasm: Vec<u8>,
        omni_token_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let worker = near_workspaces::sandbox().await?;

        let legacy_token_deployer = std::fs::read(PREV_TOKEN_DEPLOYER_WASM_FILEPATH).unwrap();
        let deployer_account = worker.dev_deploy(&legacy_token_deployer).await?;

        let legacy_init_res = deployer_account
            .call("new")
            .args_json(json!({
                "prover_account": "prover.testnet",
                "locker_address": "0000000000000000000000000000000000000000"
            }))
            .transact()
            .await?;

        assert!(
            legacy_init_res.is_success(),
            "Failed to initialize legacy contract"
        );

        let tokens: Vec<String> = deployer_account.view("get_tokens").await?.json()?;
        assert!(tokens.is_empty());

        let omni_token_code_hash = TestEnv::deploy_global_omni_token(
            &worker,
            &omni_token_wasm,
            &mock_global_contract_deployer_wasm,
        )
        .await?;
        let global_code_hash = Base58CryptoHash::from(omni_token_code_hash);

        let upgrade_res = deployer_account
            .as_account()
            .deploy(&token_deployer_wasm)
            .await?;

        assert!(upgrade_res.is_success(), "Failed to upgrade contract code");

        let migrate_res = deployer_account
            .call("migrate")
            .args_json(json!({
                "global_code_hash": global_code_hash
            }))
            .max_gas()
            .transact()
            .await?;

        assert!(
            migrate_res.is_success(),
            "Migration failed: {:?}",
            migrate_res.into_result()
        );

        let stored_global_code_hash: Base58CryptoHash = deployer_account
            .view("get_global_code_hash")
            .await?
            .json()?;

        assert_eq!(
            CryptoHash::from(stored_global_code_hash),
            omni_token_code_hash,
            "Migration did not correctly set the global token code hash"
        );

        let legacy_call_attempt = deployer_account.view("get_tokens").await;
        assert!(
            legacy_call_attempt.is_err(),
            "Legacy method should no longer exist"
        );

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_global_token_upgrade_and_migrate(
        mock_prover_wasm: Vec<u8>,
        locker_wasm: Vec<u8>,
        omni_token_wasm: Vec<u8>,
        token_deployer_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let env = TestEnv::new(
            eth_token_address(),
            mock_prover_wasm,
            locker_wasm,
            omni_token_wasm.clone(),
            token_deployer_wasm,
            mock_global_contract_deployer_wasm,
        )
        .await?;

        let recipient = env.create_registered_account(10).await?;
        let mint_amount = U128(1_000_000_000_000_000_000);

        // Ensure the token account has enough NEAR balance for code upgrade
        let root = env.worker.root_account()?;
        root.transfer_near(&env.token_account_id, NearToken::from_near(20))
            .await?
            .into_result()?;

        fake_finalize_transfer(
            &env.locker_contract,
            &env.token_account_id,
            &recipient,
            env.init_token_address,
            env.factory_contract_address,
            mint_amount,
        )
        .await?;

        let balance_before: U128 = env
            .worker
            .view(&env.token_account_id, "ft_balance_of")
            .args_json(json!({
            "account_id": recipient.id(),
            }))
            .await?
            .json()?;

        let sk = near_workspaces::types::SecretKey::from_random(
            near_workspaces::types::KeyType::ED25519,
        );
        let pk = sk.public_key();

        env.locker_contract
            .as_account()
            .call(&env.token_account_id, "attach_full_access_key")
            .args_json(json!({ "public_key": pk }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let token_account = near_workspaces::Account::from_secret_key(
            env.token_account_id.clone(),
            sk,
            &env.worker,
        );

        token_account
            .call(&env.token_account_id, "migrate")
            .args_json(json!({ "from_version": 3u32 }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let balance_after: U128 = env
            .worker
            .view(&env.token_account_id, "ft_balance_of")
            .args_json(json!({
                "account_id": recipient.id(),
            }))
            .await?
            .json()?;

        let is_using_global_token: bool = env
            .worker
            .view(&env.token_account_id, "is_using_global_token")
            .await?
            .json()?;

        assert_eq!(balance_after, balance_before);
        assert!(is_using_global_token);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_non_global_token_upgrade_and_migrate(
        omni_token_wasm: Vec<u8>,
        mock_global_contract_deployer_wasm: Vec<u8>,
    ) -> anyhow::Result<()> {
        let worker = near_workspaces::sandbox().await?;
        let root = worker.root_account()?;

        let token_account = root
            .create_subaccount("non-global-token")
            .initial_balance(NearToken::from_near(10))
            .transact()
            .await?
            .into_result()?;

        let token_contract = token_account
            .deploy(&omni_token_wasm)
            .await?
            .into_result()?;

        let metadata = BasicMetadata {
            name: "Local Token".to_string(),
            symbol: "LOC".to_string(),
            decimals: 18,
        };

        root.call(token_contract.id(), "new")
            .args_json(json!({
                "controller": root.id(),
                "metadata": metadata.clone(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let is_using_global_token: bool =
            token_contract.view("is_using_global_token").await?.json()?;

        assert!(!is_using_global_token);

        root.call(token_contract.id(), "storage_deposit")
            .args_json(json!({
                "account_id": root.id(),
                "registration_only": Some(true),
            }))
            .deposit(NEP141_DEPOSIT)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let mint_amount = U128(250);

        root.call(token_contract.id(), "mint")
            .args_json(json!({
                "account_id": root.id(),
                "amount": mint_amount,
                "msg": Option::<String>::None,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let balance_before: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": root.id(),
            }))
            .await?
            .json()?;

        let omni_token_code_hash = wasm_code_hash(&omni_token_wasm);

        let mock_global_contract_deployer = worker
            .dev_deploy(&mock_global_contract_deployer_wasm)
            .await?;

        let omni_token_global_contract_id: AccountId =
            format!("omni-token-global.{}", mock_global_contract_deployer.id()).parse()?;

        mock_global_contract_deployer
            .call("deploy_global_contract")
            .args_json((
                Base64VecU8::from(omni_token_wasm.clone()),
                &omni_token_global_contract_id,
            ))
            .max_gas()
            .deposit(
                GLOBAL_STORAGE_COST_PER_BYTE
                    .saturating_mul(omni_token_wasm.len().try_into().unwrap()),
            )
            .transact()
            .await?
            .into_result()?;

        root.call(token_contract.id(), "upgrade_and_migrate")
            .args(omni_token_code_hash.to_vec())
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let balance_after: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({
                "account_id": root.id(),
            }))
            .await?
            .json()?;

        let migrated_metadata: BasicMetadata = token_contract.view("ft_metadata").await?.json()?;

        let is_using_global_token: bool =
            token_contract.view("is_using_global_token").await?.json()?;

        assert_eq!(balance_after, balance_before);
        assert_eq!(migrated_metadata.name, metadata.name);
        assert_eq!(migrated_metadata.symbol, metadata.symbol);
        assert_eq!(migrated_metadata.decimals, metadata.decimals);
        assert!(is_using_global_token);

        Ok(())
    }

    async fn fake_finalize_transfer(
        locker_contract: &near_workspaces::Contract,
        token_account_id: &AccountId,
        recipient: &near_workspaces::Account,
        token_address: OmniAddress,
        emitter_address: OmniAddress,
        amount: U128,
    ) -> anyhow::Result<()> {
        let storage_deposit_actions = vec![StorageDepositAction {
            token_id: token_account_id.clone(),
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
