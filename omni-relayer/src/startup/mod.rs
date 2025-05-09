use anyhow::{Context, Result};
use log::info;

use evm_bridge_client::{EvmBridgeClient, EvmBridgeClientBuilder};
use near_bridge_client::NearBridgeClientBuilder;
use near_crypto::InMemorySigner;
use omni_connector::{OmniConnector, OmniConnectorBuilder};
use omni_types::ChainKind;
use solana_bridge_client::SolanaBridgeClientBuilder;
use solana_client::nonblocking::rpc_client::RpcClient;
use wormhole_bridge_client::WormholeBridgeClientBuilder;

use crate::{config, startup};

pub mod bridge_indexer;
pub mod evm;
pub mod near;
pub mod solana;

#[macro_export]
macro_rules! skip_fail {
    ($res:expr, $msg:expr, $dur:expr) => {
        match $res {
            Ok(val) => val,
            Err(err) => {
                error!("{}: {}", $msg, err);
                tokio::time::sleep(tokio::time::Duration::from_secs($dur)).await;
                continue;
            }
        }
    };
}

fn build_evm_bridge_client(
    config: &config::Config,
    chain_kind: ChainKind,
) -> Result<Option<EvmBridgeClient>> {
    let evm = match chain_kind {
        ChainKind::Eth => &config.eth,
        ChainKind::Base => &config.base,
        ChainKind::Arb => &config.arb,
        _ => unreachable!("Function `build_evm_bridge_client` supports only EVM chains"),
    };

    evm.as_ref()
        .map(|evm| {
            EvmBridgeClientBuilder::default()
                .endpoint(Some(evm.rpc_http_url.clone()))
                .chain_id(Some(evm.chain_id))
                .private_key(Some(crate::config::get_private_key(chain_kind)))
                .omni_bridge_address(Some(evm.omni_bridge_address.to_string()))
                .build()
                .context(format!("Failed to build EvmBridgeClient ({chain_kind:?})"))
        })
        .transpose()
}

pub fn build_omni_connector(
    config: &config::Config,
    near_signer: &InMemorySigner,
) -> Result<OmniConnector> {
    info!("Building Omni connector");

    let near_bridge_client = NearBridgeClientBuilder::default()
        .endpoint(Some(config.near.rpc_url.clone()))
        .private_key(Some(near_signer.secret_key.to_string()))
        .signer(Some(near_signer.account_id.to_string()))
        .omni_bridge_id(Some(config.near.omni_bridge_id.to_string()))
        .build()
        .context("Failed to build NearBridgeClient")?;

    let eth_bridge_client = build_evm_bridge_client(config, ChainKind::Eth)?;
    let base_bridge_client = build_evm_bridge_client(config, ChainKind::Base)?;
    let arb_bridge_client = build_evm_bridge_client(config, ChainKind::Arb)?;

    let solana_bridge_client = config
        .solana
        .as_ref()
        .map(|solana| {
            SolanaBridgeClientBuilder::default()
                .client(Some(RpcClient::new(solana.rpc_http_url.clone())))
                .program_id(Some(solana.program_id.parse()?))
                .wormhole_core(Some(solana.wormhole_id.parse()?))
                .keypair(Some(startup::solana::get_keypair(
                    solana.credentials_path.as_ref(),
                )))
                .build()
                .context("Failed to build SolanaBridgeClient")
        })
        .transpose()?;

    let wormhole_bridge_client = WormholeBridgeClientBuilder::default()
        .endpoint(Some(config.wormhole.api_url.clone()))
        .build()
        .context("Failed to build WormholeBridgeClient")?;

    OmniConnectorBuilder::default()
        .near_bridge_client(Some(near_bridge_client))
        .eth_bridge_client(eth_bridge_client)
        .base_bridge_client(base_bridge_client)
        .arb_bridge_client(arb_bridge_client)
        .solana_bridge_client(solana_bridge_client)
        .wormhole_bridge_client(Some(wormhole_bridge_client))
        .build()
        .context("Failed to build OmniConnector")
}
