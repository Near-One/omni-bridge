use std::{collections::HashMap, str::FromStr};

use near_sdk::{borsh, json_types::U128, serde_json::json, AccountId, NearToken};
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use omni_types::{
    locker_args::{FinTransferArgs, StorageDepositAction},
    prover_result::{InitTransferMessage, ProverResult},
    BasicMetadata, ChainKind, Fee, OmniAddress,
};

use crate::helpers::tests::{
    account_n, eth_eoa_address, eth_token_address, get_bind_token_args, locker_wasm,
    mock_prover_wasm, mock_token_wasm, token_deployer_wasm, NEP141_DEPOSIT,
};

pub struct BridgeToken {
    pub is_deployed: bool,
    pub contract: Contract,
    pub eth_address: OmniAddress,
}

pub struct TestEnvBuilder {
    worker: Worker<Sandbox>,
    pub bridge_contract: Contract,
    pub bridge_token: Option<BridgeToken>,
    pub factories: HashMap<u8, OmniAddress>,
    pub accounts: HashMap<AccountId, Account>,
    token_transfer_nonce: u64,
}

impl TestEnvBuilder {
    pub async fn new() -> anyhow::Result<Self> {
        let worker = near_workspaces::sandbox().await?;
        let prover_contract = worker.dev_deploy(&mock_prover_wasm()).await?;
        let bridge_contract = worker.dev_deploy(&locker_wasm()).await?;

        bridge_contract
            .call("new")
            .args_json(json!({
                "prover_account": prover_contract.id(),
                "mpc_signer": "mpc.testnet",
                "nonce": U128(0),
                "wnear_account_id": "wnear.testnet",
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(Self {
            worker,
            bridge_contract,
            bridge_token: None,
            factories: HashMap::new(),
            accounts: HashMap::new(),
            token_transfer_nonce: 1,
        })
    }

    pub async fn add_factory(&mut self, factory_address: OmniAddress) -> anyhow::Result<()> {
        self.bridge_contract
            .call("add_factory")
            .args_json(json!({
                "address": factory_address,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        self.factories
            .insert(factory_address.get_chain().into(), factory_address);

        Ok(())
    }

    pub async fn deploy_token_deployer(&mut self, chain: ChainKind) -> anyhow::Result<()> {
        let token_deployer = self
            .worker
            .create_tla_and_deploy(
                account_n(9),
                self.worker.dev_generate().await.1,
                &token_deployer_wasm(),
            )
            .await?
            .unwrap();

        token_deployer
            .call("new")
            .args_json(json!({
                "controller": self.bridge_contract.id(),
                "dao": AccountId::from_str("dao.near").unwrap(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        self.bridge_contract
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

    pub async fn create_account(&mut self, id: AccountId) -> anyhow::Result<()> {
        let account = self
            .worker
            .create_tla(id.clone(), self.worker.dev_generate().await.1)
            .await?
            .unwrap();
        self.accounts.insert(id, account);
        Ok(())
    }

    pub async fn deploy_eth_token(&mut self) -> anyhow::Result<()> {
        if self.bridge_token.is_some() {
            anyhow::bail!("Token contract already deployed");
        }

        let init_token_address = OmniAddress::new_zero(ChainKind::Eth).unwrap();
        let token_metadata = BasicMetadata {
            name: "ETH from Ethereum".to_string(),
            symbol: "ETH".to_string(),
            decimals: 18,
        };

        let required_storage: NearToken = self
            .bridge_contract
            .view("required_balance_for_deploy_token")
            .await?
            .json()?;

        self.bridge_contract
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

        let token_account_id: AccountId = self
            .bridge_contract
            .view("get_token_id")
            .args_json(json!({
                "address": init_token_address
            }))
            .await?
            .json()?;

        let token_contract = self
            .worker
            .import_contract(&token_account_id, &self.worker)
            .transact()
            .await?;

        self.bridge_token = Some(BridgeToken {
            is_deployed: true,
            contract: token_contract,
            eth_address: init_token_address,
        });

        Ok(())
    }

    pub async fn deploy_native_nep141_token(&mut self) -> anyhow::Result<()> {
        if self.bridge_token.is_some() {
            anyhow::bail!("Token contract already deployed");
        }

        let token_contract = self.worker.dev_deploy(&mock_token_wasm()).await?;
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

        let required_deposit_for_bind_token = self
            .bridge_contract
            .view("required_balance_for_bind_token")
            .await?
            .json()?;

        self.bridge_contract
            .call("bind_token")
            .args_borsh(get_bind_token_args(
                token_contract.id(),
                &eth_token_address(),
                &self.factories[&ChainKind::Eth.into()].clone(),
                18,
                24,
            ))
            .deposit(required_deposit_for_bind_token)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        self.bridge_token = Some(BridgeToken {
            is_deployed: false,
            contract: token_contract,
            eth_address: eth_token_address(),
        });

        Ok(())
    }

    pub async fn mint_tokens(&mut self, recipient: &AccountId, amount: u128) -> anyhow::Result<()> {
        if self.bridge_token.is_none() {
            anyhow::bail!("Token contract not deployed");
        }

        let bridge_token = self.bridge_token.as_ref().unwrap();

        if !bridge_token.is_deployed {
            bridge_token
                .contract
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": recipient.clone(),
                    "registration_only": true,
                }))
                .deposit(NEP141_DEPOSIT)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            bridge_token
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
        } else {
            let storage_deposit_actions = vec![StorageDepositAction {
                token_id: bridge_token.contract.id().clone(),
                account_id: recipient.clone(),
                storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
            }];

            let required_balance_for_fin_transfer: NearToken = self
                .bridge_contract
                .view("required_balance_for_fin_transfer")
                .await?
                .json()?;
            let required_deposit_for_fin_transfer =
                NEP141_DEPOSIT.saturating_add(required_balance_for_fin_transfer);

            // Simulate finalization of transfer through locker
            self.bridge_contract
                .call("fin_transfer")
                .args_borsh(FinTransferArgs {
                    chain_kind: ChainKind::Near,
                    storage_deposit_actions,
                    prover_args: borsh::to_vec(&ProverResult::InitTransfer(InitTransferMessage {
                        origin_nonce: self.token_transfer_nonce,
                        token: bridge_token.eth_address.clone(),
                        recipient: OmniAddress::Near(recipient.clone()),
                        amount: U128(amount),
                        fee: Fee {
                            fee: U128(0),
                            native_fee: U128(0),
                        },
                        sender: eth_eoa_address(),
                        msg: String::default(),
                        emitter_address: self.factories[&ChainKind::Eth.into()].clone(),
                    }))?,
                })
                .deposit(required_deposit_for_fin_transfer)
                .max_gas()
                .transact()
                .await?
                .into_result()?;

            self.token_transfer_nonce += 1;
        }

        Ok(())
    }
}
