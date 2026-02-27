use anyhow::{Context, Result};
use async_trait::async_trait;
use near_api::{AccountId, Contract, Data, NetworkConfig, types::json::U128};
use omni_types::{ChainKind, OmniAddress};
use serde_json::json;

#[derive(Clone)]
pub struct Client {
    omni_bridge: Contract,
    network: NetworkConfig,
}

impl Client {
    pub fn new(omni_bridge_account_id: AccountId, rpc_url: &str) -> Result<Self> {
        let network =
            NetworkConfig::from_rpc_url("client", rpc_url.parse().context("Invalid NEAR RPC URL")?);

        Ok(Self {
            omni_bridge: Contract(omni_bridge_account_id),
            network,
        })
    }

    pub async fn get_bridged_token(
        &self,
        token_address: &OmniAddress,
        chain: ChainKind,
    ) -> Result<OmniAddress> {
        let token_id = self
            .omni_bridge
            .call_function(
                "get_bridged_token",
                json!({
                    "address": token_address,
                    "chain": chain
                }),
            )
            .read_only()
            .fetch_from(&self.network)
            .await
            .context(format!(
                "Failed to fetch bridged token ({token_address}) from OmniBridge"
            ))?;

        Ok(token_id.data)
    }
}

#[async_trait]
impl super::Client for Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<u128> {
        let token_id = match token_address {
            OmniAddress::Near(token_id) => token_id,
            address => {
                let OmniAddress::Near(token_id) =
                    self.get_bridged_token(&address, ChainKind::Near).await?
                else {
                    unreachable!("Unexpected address type");
                };

                token_id
            }
        };

        let total_supply: Data<U128> = Contract(token_id)
            .call_function("ft_total_supply", ())
            .read_only()
            .fetch_from(&self.network)
            .await?;

        Ok(total_supply.data.0)
    }
}
