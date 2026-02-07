use anyhow::{Context, Result};
use async_trait::async_trait;
use omni_types::{ChainKind, H160, OmniAddress};
use std::sync::Arc;

use alloy::{
    primitives::Address,
    providers::{DynProvider, ProviderBuilder},
    sol,
};

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function totalSupply() external view returns (uint256 totalSupply);
    }
}

pub struct Client {
    near_client: Arc<super::near::Client>,
    provider: DynProvider,
    chain: ChainKind,
}

impl Client {
    pub fn new(
        near_client: Arc<super::near::Client>,
        rpc_http_url: String,
        chain: ChainKind,
    ) -> Result<Self> {
        if !chain.is_evm_chain() {
            anyhow::bail!("Unsupported chain kind: {chain:?}");
        }

        let rpc_http_url = rpc_http_url
            .parse()
            .context(format!("Failed to parse {chain:?} RPC HTTP URL"))?;
        let provider = DynProvider::new(ProviderBuilder::new().connect_http(rpc_http_url));

        Ok(Self {
            near_client,
            provider,
            chain,
        })
    }

    fn match_evm_address(&self, address: OmniAddress) -> Result<H160> {
        match (self.chain, address) {
            (ChainKind::Eth, OmniAddress::Eth(address)) => Ok(address),
            (ChainKind::Arb, OmniAddress::Arb(address)) => Ok(address),
            (ChainKind::Base, OmniAddress::Base(address)) => Ok(address),
            (ChainKind::Bnb, OmniAddress::Bnb(address)) => Ok(address),
            (ChainKind::Pol, OmniAddress::Pol(address)) => Ok(address),
            (chain, address) => {
                anyhow::bail!("Unexpected address type ({address}) for {chain:?} chain")
            }
        }
    }
}

#[async_trait]
impl super::Client for Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<u128> {
        let token_address = match token_address {
            OmniAddress::Eth(address) if self.chain == ChainKind::Eth => address,
            OmniAddress::Arb(address) if self.chain == ChainKind::Arb => address,
            OmniAddress::Base(address) if self.chain == ChainKind::Base => address,
            OmniAddress::Bnb(address) if self.chain == ChainKind::Bnb => address,
            OmniAddress::Pol(address) if self.chain == ChainKind::Pol => address,
            address => {
                let bridged = self
                    .near_client
                    .get_bridged_token(&address, self.chain)
                    .await?;
                self.match_evm_address(bridged)?
            }
        };

        let address = Address::from_slice(&token_address.0);
        let erc20 = IERC20::new(address, &self.provider);
        let total_supply = erc20.totalSupply().call().await.context(format!(
            "Failed to fetch total supply for token {token_address} on {:?}",
            self.chain
        ))?;
        let parsed_total_supply = total_supply.try_into().context(format!(
            "Failed to parse total supply for token {token_address} on {:?}",
            self.chain
        ))?;

        Ok(parsed_total_supply)
    }
}
