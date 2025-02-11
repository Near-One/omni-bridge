use std::sync::Mutex;

use alloy::{
    primitives::{Address, U64},
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::http::Http,
};
use anyhow::Result;
use log::warn;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::BlockReference;
use omni_types::ChainKind;
use reqwest::Client;

use crate::config;

const RETRY_ATTEMPTS: u64 = 10;
const RETRY_SLEEP_SECS: u64 = 1;

pub enum ChainClient {
    Near {
        jsonrpc_client: JsonRpcClient,
        signer: InMemorySigner,
    },
    Evm {
        provider: RootProvider<Http<Client>>,
        address: Address,
    },
}

pub struct NonceManager {
    nonce: Mutex<u64>,
    client: ChainClient,
}

impl NonceManager {
    pub fn new(client: ChainClient) -> Self {
        Self {
            nonce: Mutex::new(0),
            client,
        }
    }

    pub async fn resync_nonce(&self) -> Result<()> {
        let current_nonce = self.get_current_nonce().await?;
        let mut local_nonce = self
            .nonce
            .lock()
            .map_err(|_| anyhow::anyhow!("Mutex lock error during nonce update"))?;
        *local_nonce = current_nonce;

        Ok(())
    }

    pub async fn reserve_nonce(&self) -> Result<u64> {
        let current_nonce = self.get_current_nonce().await?;

        let mut local_nonce = self
            .nonce
            .lock()
            .map_err(|_| anyhow::anyhow!("Mutex lock error during nonce update"))?;

        if *local_nonce < current_nonce {
            *local_nonce = current_nonce;
        }

        let reserved = *local_nonce;
        *local_nonce += 1;

        Ok(reserved)
    }

    pub async fn get_current_nonce(&self) -> Result<u64> {
        match &self.client {
            ChainClient::Near {
                jsonrpc_client,
                signer,
            } => Self::get_current_nonce_near(jsonrpc_client, signer).await,
            ChainClient::Evm { provider, address } => {
                Self::get_current_nonce_evm(provider.clone(), *address).await
            }
        }
    }

    async fn get_current_nonce_near(
        jsonrpc_client: &JsonRpcClient,
        signer: &InMemorySigner,
    ) -> Result<u64> {
        let rpc_request = methods::query::RpcQueryRequest {
            block_reference: BlockReference::latest(),
            request: near_primitives::views::QueryRequest::ViewAccessKey {
                account_id: signer.account_id.clone(),
                public_key: signer.public_key.clone(),
            },
        };

        for _ in 0..RETRY_ATTEMPTS {
            let access_key_query_response = jsonrpc_client.call(&rpc_request).await?;

            let QueryResponseKind::AccessKey(access_key) = access_key_query_response.kind else {
                warn!("Failed to get access key, retrying...");
                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_SLEEP_SECS)).await;
                continue;
            };

            return Ok(access_key.nonce + 1);
        }

        anyhow::bail!("Failed to get current nonce")
    }

    async fn get_current_nonce_evm(
        provider: RootProvider<Http<Client>>,
        address: Address,
    ) -> Result<u64> {
        for _ in 0..RETRY_ATTEMPTS {
            let response = provider
                .client()
                .request("eth_getTransactionCount", (address, "pending"))
                .map_resp(|x: U64| x.to::<u64>())
                .await;

            let Ok(nonce) = response else {
                warn!("Failed to get transaction count, retrying...");
                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_SLEEP_SECS)).await;
                continue;
            };

            return Ok(nonce);
        }

        anyhow::bail!("Failed to get current nonce")
    }
}

pub struct EvmNonceManagers {
    pub eth: Option<NonceManager>,
    pub base: Option<NonceManager>,
    pub arb: Option<NonceManager>,
}

impl EvmNonceManagers {
    pub fn new(config: &config::Config) -> Self {
        Self {
            eth: config.eth.as_ref().map(|eth_config| {
                NonceManager::new(ChainClient::Evm {
                    provider: ProviderBuilder::new()
                        .on_http(eth_config.rpc_http_url.parse().unwrap()),
                    address: config::get_evm_address(ChainKind::Eth),
                })
            }),
            base: config.base.as_ref().map(|base_config| {
                NonceManager::new(ChainClient::Evm {
                    provider: ProviderBuilder::new()
                        .on_http(base_config.rpc_http_url.parse().unwrap()),
                    address: config::get_evm_address(ChainKind::Base),
                })
            }),
            arb: config.arb.as_ref().map(|arb_config| {
                NonceManager::new(ChainClient::Evm {
                    provider: ProviderBuilder::new()
                        .on_http(arb_config.rpc_http_url.parse().unwrap()),
                    address: config::get_evm_address(ChainKind::Arb),
                })
            }),
        }
    }

    pub async fn reserve_nonce(&self, chain_kind: ChainKind) -> Result<u64> {
        match chain_kind {
            ChainKind::Eth => {
                self.eth
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Eth nonce manager is not initialized"))?
                    .reserve_nonce()
                    .await
            }
            ChainKind::Base => {
                self.base
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Base nonce manager is not initialized"))?
                    .reserve_nonce()
                    .await
            }
            ChainKind::Arb => {
                self.arb
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Arb nonce manager is not initialized"))?
                    .reserve_nonce()
                    .await
            }
            _ => anyhow::bail!("Unsupported chain kind: {chain_kind:?}"),
        }
    }
}
