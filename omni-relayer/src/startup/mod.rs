use anyhow::{Context, Result};
use log::info;

use near_crypto::InMemorySigner;

use evm_bridge_client::EvmBridgeClientBuilder;
use near_bridge_client::NearBridgeClientBuilder;
use omni_connector::{OmniConnector, OmniConnectorBuilder};
use omni_types::ChainKind;
use wormhole_bridge_client::WormholeBridgeClientBuilder;

use crate::config;

pub mod evm;
pub mod near;

pub fn build_omni_connector(
    config: &config::Config,
    near_signer: &InMemorySigner,
) -> Result<OmniConnector> {
    info!("Building Omni connector");

    let near_bridge_client = NearBridgeClientBuilder::default()
        .endpoint(Some(config.near.rpc_url.clone()))
        .private_key(Some(near_signer.secret_key.to_string()))
        .signer(Some(near_signer.account_id.to_string()))
        .token_locker_id(Some(config.near.token_locker_id.to_string()))
        .build()
        .context("Failed to build NearBridgeClient")?;

    let eth_bridge_client = if let Some(ref eth) = config.eth {
        Some(
            EvmBridgeClientBuilder::default()
                .endpoint(Some(eth.rpc_http_url.clone()))
                .chain_id(Some(eth.chain_id))
                .private_key(Some(crate::config::get_evm_private_key(ChainKind::Eth)))
                .bridge_token_factory_address(Some(eth.bridge_token_factory_address.to_string()))
                .build()
                .context("Failed to build EvmBridgeClient (Eth)")?,
        )
    } else {
        None
    };

    let base_bridge_client = if let Some(ref base) = config.base {
        Some(
            EvmBridgeClientBuilder::default()
                .endpoint(Some(base.rpc_http_url.clone()))
                .chain_id(Some(base.chain_id))
                .private_key(Some(crate::config::get_evm_private_key(ChainKind::Base)))
                .bridge_token_factory_address(Some(base.bridge_token_factory_address.to_string()))
                .build()
                .context("Failed to build EvmBridgeClient (Base)")?,
        )
    } else {
        None
    };

    let arb_bridge_client = if let Some(ref arb) = config.arb {
        Some(
            EvmBridgeClientBuilder::default()
                .endpoint(Some(arb.rpc_http_url.clone()))
                .chain_id(Some(arb.chain_id))
                .private_key(Some(crate::config::get_evm_private_key(ChainKind::Arb)))
                .bridge_token_factory_address(Some(arb.bridge_token_factory_address.to_string()))
                .build()
                .context("Failed to build EvmBridgeClient (Arb)")?,
        )
    } else {
        None
    };

    let wormhole_bridge_client = WormholeBridgeClientBuilder::default()
        .endpoint(Some(config.wormhole.api_url.clone()))
        .build()
        .context("Failed to build WormholeBridgeClient")?;

    OmniConnectorBuilder::default()
        .near_bridge_client(Some(near_bridge_client))
        .eth_bridge_client(eth_bridge_client)
        .base_bridge_client(base_bridge_client)
        .arb_bridge_client(arb_bridge_client)
        .solana_bridge_client(None)
        .wormhole_bridge_client(Some(wormhole_bridge_client))
        .build()
        .context("Failed to build OmniConnector")
}
