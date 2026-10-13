use anyhow::{Context, Result};
use async_trait::async_trait;
use omni_types::{ChainKind, OmniAddress};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

pub struct Client {
    near_client: Arc<super::near::Client>,
    rpc_client: RpcClient,
}

impl Client {
    pub fn new(near_client: Arc<super::near::Client>, rpc_http_url: String) -> Self {
        Self {
            near_client,
            rpc_client: RpcClient::new(rpc_http_url),
        }
    }
}

#[async_trait]
impl super::Client for Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<u128> {
        let token_address = match token_address {
            OmniAddress::Sol(token_address) => token_address,
            address => {
                let OmniAddress::Sol(token_address) = self
                    .near_client
                    .get_bridged_token(&address, ChainKind::Sol)
                    .await?
                else {
                    unreachable!("Unexpected address type");
                };

                token_address
            }
        };

        let mint = Pubkey::new_from_array(token_address.0);
        let supply = self
            .rpc_client
            .get_token_supply(&mint)
            .await
            .context("Failed to get token supply on Solana")?;
        let total_supply = supply
            .amount
            .parse::<u128>()
            .context("Failed to parse total supply on Solana")?;

        Ok(total_supply)
    }
}
