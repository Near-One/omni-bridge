use anyhow::Result;
use async_trait::async_trait;
use omni_types::OmniAddress;
use std::sync::Arc;

pub mod evm;
pub mod near;
pub mod solana;

#[async_trait]
pub trait Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<u128>;
}

pub struct Clients {
    pub near: Arc<near::Client>,
    pub eth: evm::Client,
    pub base: evm::Client,
    pub arb: evm::Client,
    pub bnb: evm::Client,
    pub pol: evm::Client,
    pub solana: solana::Client,
}

impl Clients {
    pub fn new(
        near_client: Arc<near::Client>,
        eth_rpc_url: String,
        base_rpc_url: String,
        arb_rpc_url: String,
        bnb_rpc_url: String,
        pol_rpc_url: String,
        solana_rpc_url: String,
    ) -> Result<Self> {
        Ok(Self {
            near: Arc::clone(&near_client),
            eth: evm::Client::new(
                Arc::clone(&near_client),
                eth_rpc_url,
                omni_types::ChainKind::Eth,
            )?,
            base: evm::Client::new(
                Arc::clone(&near_client),
                base_rpc_url,
                omni_types::ChainKind::Base,
            )?,
            arb: evm::Client::new(
                Arc::clone(&near_client),
                arb_rpc_url,
                omni_types::ChainKind::Arb,
            )?,
            bnb: evm::Client::new(
                Arc::clone(&near_client),
                bnb_rpc_url,
                omni_types::ChainKind::Bnb,
            )?,
            pol: evm::Client::new(
                Arc::clone(&near_client),
                pol_rpc_url,
                omni_types::ChainKind::Pol,
            )?,
            solana: solana::Client::new(Arc::clone(&near_client), solana_rpc_url),
        })
    }
}
