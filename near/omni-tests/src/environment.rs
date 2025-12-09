use std::{cell::RefCell, str::FromStr};

use anyhow::Ok;
use near_sdk::{
    borsh,
    json_types::U128,
    serde_json::{self, json},
    AccountId, NearToken,
};
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use omni_types::{
    locker_args::{FinTransferArgs, StorageDepositAction},
    prover_result::{InitTransferMessage, ProverResult},
    BasicMetadata, ChainKind, Fee, OmniAddress,
};

use crate::helpers::tests::{
    account_n, eth_eoa_address, eth_factory_address, eth_token_address, get_bind_token_args,
    get_test_deploy_token_args, BuildArtifacts, NEP141_DEPOSIT,
};

const PREV_LOCKER_WASM_FILEPATH: &str = "src/data/omni_bridge-0_3_2.wasm";
const DEFAULT_LOCKED_TOKENS: u128 = 1_000_000_000_000_000_000_000_000;

pub struct BridgeToken {
    pub is_deployed: bool,
    pub contract: Contract,
    pub eth_address: OmniAddress,
}

pub struct TestEnvBuilder {
    worker: Worker<Sandbox>,
    build_artifacts: BuildArtifacts,
    deploy_old_version: bool,
}

pub struct TestEnvBuilderWithToken {
    pub worker: Worker<Sandbox>,
    pub bridge_contract: Contract,
    pub token: BridgeToken,
    pub utxo_connector: Option<Contract>,
    build_artifacts: BuildArtifacts,
    token_transfer_nonce: RefCell<u64>,
}

impl TestEnvBuilder {
    pub async fn new(build_artifacts: BuildArtifacts) -> anyhow::Result<Self> {
        let worker = near_workspaces::sandbox().await?;

        Ok(Self {
            worker,
            build_artifacts,
            deploy_old_version: false,
        })
    }

    pub fn deploy_old_version(mut self, deploy: bool) -> Self {
        self.deploy_old_version = deploy;
        self
    }

    pub async fn with_custom_wnear(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let token_contract = self.deploy_nep141_token().await?;

        let bridge_contract = self
            .deploy_bridge(Some(token_contract.id().clone()))
            .await?;

        add_factory(&bridge_contract, eth_factory_address()).await?;

        bind_token(
            &bridge_contract,
            &eth_token_address(),
            &eth_factory_address(),
            &token_contract,
            24,
        )
        .await?;

        storage_deposit(&token_contract, bridge_contract.id()).await?;
        seed_locked_tokens(&bridge_contract, token_contract.id()).await?;

        Ok(TestEnvBuilderWithToken {
            worker: self.worker,
            bridge_contract,
            token: BridgeToken {
                is_deployed: false,
                contract: token_contract,
                eth_address: eth_token_address(),
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_native_nep141_token(
        self,
        destination_decimals: u8,
    ) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;

        let token_contract = self.deploy_nep141_token().await?;

        add_factory(&bridge_contract, eth_factory_address()).await?;

        bind_token(
            &bridge_contract,
            &eth_token_address(),
            &eth_factory_address(),
            &token_contract,
            destination_decimals,
        )
        .await?;

        storage_deposit(&token_contract, bridge_contract.id()).await?;
        seed_locked_tokens(&bridge_contract, token_contract.id()).await?;

        Ok(TestEnvBuilderWithToken {
            worker: self.worker,
            bridge_contract,
            token: BridgeToken {
                is_deployed: false,
                contract: token_contract,
                eth_address: eth_token_address(),
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_bridged_eth(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;
        self.deploy_token_deployer(&bridge_contract, ChainKind::Eth)
            .await?;

        add_factory(&bridge_contract, eth_factory_address()).await?;

        let init_token_address = OmniAddress::new_zero(ChainKind::Eth).unwrap();
        let token_metadata = BasicMetadata {
            name: "ETH from Ethereum".to_string(),
            symbol: "ETH".to_string(),
            decimals: 18,
        };

        let required_storage: NearToken = bridge_contract
            .view("required_balance_for_deploy_token")
            .await?
            .json()?;

        bridge_contract
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

        let token_contract = self
            .get_token_contract(&bridge_contract, &init_token_address)
            .await?;

        storage_deposit(&token_contract, bridge_contract.id()).await?;

        Ok(TestEnvBuilderWithToken {
            worker: self.worker,
            bridge_contract,
            token: BridgeToken {
                is_deployed: true,
                contract: token_contract,
                eth_address: init_token_address,
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_bridged_token(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;

        self.deploy_token_deployer(&bridge_contract, ChainKind::Eth)
            .await?;

        add_factory(&bridge_contract, eth_factory_address()).await?;

        let token_deploy_initiator = self
            .worker
            .create_tla(account_n(2), self.worker.dev_generate().await.1)
            .await?
            .unwrap();

        let required_storage: NearToken = bridge_contract
            .view("required_balance_for_deploy_token")
            .await?
            .json()?;

        token_deploy_initiator
            .call(bridge_contract.id(), "deploy_token")
            .args_borsh(get_test_deploy_token_args(
                &eth_token_address(),
                &eth_factory_address(),
                &BasicMetadata {
                    name: "Test Token".to_string(),
                    symbol: "TEST".to_string(),
                    decimals: 18,
                },
            ))
            .deposit(required_storage)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let token_contract = self
            .get_token_contract(&bridge_contract, &eth_token_address())
            .await?;

        storage_deposit(&token_contract, bridge_contract.id()).await?;

        Ok(TestEnvBuilderWithToken {
            worker: self.worker,
            bridge_contract,
            token: BridgeToken {
                is_deployed: true,
                contract: token_contract,
                eth_address: eth_token_address(),
            },
            utxo_connector: None,
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    pub async fn with_utxo_token(self) -> anyhow::Result<TestEnvBuilderWithToken> {
        let bridge_contract = self.deploy_bridge(None).await?;

        let token_contract = self.deploy_nep141_token().await?;

        let utxo_connector = self
            .worker
            .dev_deploy(&self.build_artifacts.mock_utxo_connector)
            .await?;

        utxo_connector
            .call("new")
            .args_json(json!({
                "bridge_account": bridge_contract.id(),
                "token_account": token_contract.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        bridge_contract
            .call("add_utxo_chain_connector")
            .args_json(json!({
                "chain_kind": ChainKind::Btc,
                "utxo_chain_connector_id": utxo_connector.id(),
                "utxo_chain_token_id": token_contract.id(),
                "decimals": 8,
            }))
            .deposit(NEP141_DEPOSIT.saturating_mul(3))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        storage_deposit(&token_contract, bridge_contract.id()).await?;
        storage_deposit(&token_contract, utxo_connector.id()).await?;

        // Transfer some NEAR to the connector for making cross-contract calls
        self.worker
            .root_account()?
            .transfer_near(utxo_connector.id(), NearToken::from_yoctonear(1000))
            .await?
            .into_result()?;

        Ok(TestEnvBuilderWithToken {
            worker: self.worker,
            bridge_contract,
            token: BridgeToken {
                is_deployed: false,
                contract: token_contract,
                eth_address: OmniAddress::new_zero(ChainKind::Btc).unwrap(),
            },
            utxo_connector: Some(utxo_connector),
            build_artifacts: self.build_artifacts,
            token_transfer_nonce: RefCell::new(1),
        })
    }

    async fn deploy_token_deployer(
        &self,
        bridge_contract: &Contract,
        chain: ChainKind,
    ) -> anyhow::Result<()> {
        let token_deployer = self
            .worker
            .create_tla_and_deploy(
                account_n(9),
                self.worker.dev_generate().await.1,
                &self.build_artifacts.token_deployer,
            )
            .await?
            .unwrap();

        token_deployer
            .call("new")
            .args_json(json!({
                "controller": bridge_contract.id(),
                "dao": AccountId::from_str("dao.near").unwrap(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        bridge_contract
            .call("add_token_deployer")
            .args_json(json!({
                "chain": chain,
                "account_id": token_deployer.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(())
    }

    async fn deploy_bridge(&self, wnear_account_id: Option<AccountId>) -> anyhow::Result<Contract> {
        let locker_wasm = if self.deploy_old_version {
            &std::fs::read(PREV_LOCKER_WASM_FILEPATH).unwrap()
        } else {
            &self.build_artifacts.locker
        };

        let prover_contract = self
            .worker
            .dev_deploy(&self.build_artifacts.mock_prover)
            .await?;
        let bridge_contract = self.worker.dev_deploy(locker_wasm).await?;

        let mut args = serde_json::Map::new();
        args.insert("mpc_signer".to_string(), json!("mpc.testnet"));
        args.insert("nonce".to_string(), json!(U128(0)));
        args.insert(
            "wnear_account_id".to_string(),
            json!(wnear_account_id.unwrap_or("wnear.testnet".parse().unwrap())),
        );

        bridge_contract
            .call("new")
            .args_json(json!(args))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        bridge_contract
            .call("add_prover")
            .args_json(json!({
                "chain": "Eth",
                "account_id": prover_contract.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(bridge_contract)
    }

    async fn deploy_nep141_token(&self) -> anyhow::Result<Contract> {
        let token_contract = self
            .worker
            .dev_deploy(&self.build_artifacts.mock_token)
            .await?;
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

        Ok(token_contract)
    }

    async fn get_token_contract(
        &self,
        bridge_contract: &Contract,
        token_address: &OmniAddress,
    ) -> anyhow::Result<Contract> {
        let token_account_id: AccountId = bridge_contract
            .view("get_token_id")
            .args_json(json!({
                "address": token_address
            }))
            .await?
            .json()?;

        let token_contract = self
            .worker
            .import_contract(&token_account_id, &self.worker)
            .transact()
            .await?;

        Ok(token_contract)
    }
}

impl TestEnvBuilderWithToken {
    pub async fn storage_deposit(&self, account_id: &AccountId) -> anyhow::Result<()> {
        storage_deposit(&self.token.contract, account_id).await?;

        Ok(())
    }

    pub async fn omni_storage_deposit(
        &self,
        account_id: &AccountId,
        amount: u128,
    ) -> anyhow::Result<()> {
        self.bridge_contract
            .call("storage_deposit")
            .args_json(json!({
                "account_id": account_id,
            }))
            .deposit(NearToken::from_yoctonear(amount))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(())
    }

    pub async fn mint_tokens(&self, recipient: &AccountId, amount: u128) -> anyhow::Result<()> {
        if self.token.is_deployed {
            let storage_deposit_actions = vec![StorageDepositAction {
                token_id: self.token.contract.id().clone(),
                account_id: recipient.clone(),
                storage_deposit_amount: None,
            }];

            let required_deposit_for_fin_transfer: NearToken = self
                .bridge_contract
                .view("required_balance_for_fin_transfer")
                .await?
                .json()?;

            // Simulate finalization of transfer through locker
            self.bridge_contract
                .call("fin_transfer")
                .args_borsh(FinTransferArgs {
                    chain_kind: ChainKind::Eth,
                    storage_deposit_actions,
                    prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                        origin_nonce: {
                            let current_nonce = *self.token_transfer_nonce.borrow();
                            current_nonce
                        },
                        token: self.token.eth_address.clone(),
                        recipient: OmniAddress::Near(recipient.clone()),
                        amount: U128(amount),
                        fee: Fee {
                            fee: U128(0),
                            native_fee: U128(0),
                        },
                        sender: eth_eoa_address(),
                        msg: String::default(),
                        emitter_address: eth_factory_address(),
                    }))?,
                })
                .deposit(required_deposit_for_fin_transfer)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            *self.token_transfer_nonce.borrow_mut() += 1;
        } else {
            self.token
                .contract
                .call("ft_transfer")
                .args_json(json!({
                    "receiver_id": recipient.clone(),
                    "amount": U128(amount),
                    "memo": None::<String>,
                }))
                .deposit(NearToken::from_yoctonear(1))
                .max_gas()
                .transact()
                .await?
                .into_result()?;
        }

        Ok(())
    }

    pub async fn create_account(&self, id: AccountId) -> anyhow::Result<Account> {
        let account = self
            .worker
            .create_tla(id.clone(), self.worker.dev_generate().await.1)
            .await?
            .unwrap();
        Ok(account)
    }

    pub async fn deploy_mock_receiver(&self) -> anyhow::Result<Contract> {
        let token_receiver = self
            .worker
            .dev_deploy(&self.build_artifacts.mock_token_receiver)
            .await?;
        Ok(token_receiver)
    }
}

async fn bind_token(
    bridge_contract: &Contract,
    token_address: &OmniAddress,
    factory_address: &OmniAddress,
    token_contract: &Contract,
    destination_decimals: u8,
) -> anyhow::Result<()> {
    let required_deposit_for_bind_token = bridge_contract
        .view("required_balance_for_bind_token")
        .await?
        .json()?;

    bridge_contract
        .call("bind_token")
        .args_borsh(get_bind_token_args(
            token_contract.id(),
            token_address,
            factory_address,
            destination_decimals,
            24,
        ))
        .deposit(required_deposit_for_bind_token)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

async fn add_factory(
    bridge_contract: &Contract,
    factory_address: OmniAddress,
) -> anyhow::Result<()> {
    bridge_contract
        .call("add_factory")
        .args_json(json!({
            "address": factory_address,
        }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

async fn storage_deposit(token_contract: &Contract, account_id: &AccountId) -> anyhow::Result<()> {
    token_contract
        .call("storage_deposit")
        .args_json(json!({
            "account_id": account_id,
            "registration_only": true,
        }))
        .deposit(NEP141_DEPOSIT)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

async fn seed_locked_tokens(bridge_contract: &Contract, token_id: &AccountId) -> anyhow::Result<()> {
    bridge_contract
        .call("set_locked_tokens")
        .args_json(json!({
            "chain_kind": ChainKind::Eth,
            "token_id": token_id,
            "amount": U128(DEFAULT_LOCKED_TOKENS),
        }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    Ok(())
}
