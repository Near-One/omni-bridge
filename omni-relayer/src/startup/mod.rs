use anyhow::{Context, Result};
use log::info;

use near_crypto::InMemorySigner;
use omni_connector::{OmniConnector, OmniConnectorBuilder};

use crate::config;

pub mod eth;
pub mod near;

pub fn build_connector(
    config: &config::Config,
    near_signer: &InMemorySigner,
) -> Result<OmniConnector> {
    info!("Building NEP-141 connector");

    OmniConnectorBuilder::default()
        .eth_endpoint(Some(config.eth.rpc_http_url.clone()))
        .eth_chain_id(Some(config.eth.chain_id))
        .near_endpoint(Some(config.near.rpc_url.clone()))
        .token_locker_id(Some(config.near.token_locker_id.to_string()))
        .bridge_token_factory_address(Some(config.eth.bridge_token_factory_address.to_string()))
        .eth_private_key(Some(
            std::env::var("ETH_PRIVATE_KEY")
                .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?,
        ))
        .near_signer(Some(near_signer.account_id.to_string()))
        .near_private_key(Some(near_signer.secret_key.to_string()))
        .build()
        .context("Failed to build Nep141Connector")
}
