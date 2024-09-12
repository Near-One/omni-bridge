use anyhow::{Context, Result};
use log::info;

use near_crypto::InMemorySigner;

use crate::defaults;

pub mod eth;
pub mod near;

pub fn build_connector(
    config: crate::Config,
    near_signer: &InMemorySigner,
) -> Result<nep141_connector::Nep141Connector> {
    info!("Building NEP-141 connector");

    nep141_connector::Nep141ConnectorBuilder::default()
        .eth_endpoint(Some(defaults::ETH_RPC_TESTNET.to_string()))
        .eth_chain_id(Some(defaults::ETH_CHAIN_ID_TESTNET))
        .near_endpoint(Some(defaults::NEAR_RPC_TESTNET.to_string()))
        .token_locker_id(Some(config.token_locker_id_testnet))
        .bridge_token_factory_address(Some(config.bridge_token_factory_address_testnet))
        .near_light_client_address(Some(config.near_light_client_eth_address_testnet))
        .eth_private_key(Some(
            std::env::var("ETH_PRIVATE_KEY")
                .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?,
        ))
        .near_signer(Some(near_signer.account_id.to_string()))
        .near_private_key(Some(near_signer.secret_key.to_string()))
        .build()
        .context("Failed to build Nep141Connector")
}
