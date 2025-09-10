use std::collections::HashMap;

use anyhow::{Context, Result};
use bridge_indexer_types::documents_types::UtxoChain;
use eth_light_client::{EthLightClient, EthLightClientBuilder};
use evm_bridge_client::{EvmBridgeClient, EvmBridgeClientBuilder};
use near_bridge_client::{NearBridgeClientBuilder, UTXOChainAccounts};
use near_crypto::InMemorySigner;
use omni_connector::{OmniConnector, OmniConnectorBuilder};
use omni_types::ChainKind;
use solana_bridge_client::{SolanaBridgeClient, SolanaBridgeClientBuilder};
use solana_client::nonblocking::rpc_client::RpcClient;
use tracing::{info, warn};
use utxo_bridge_client::{AuthOptions, UTXOBridgeClient};
use utxo_utils::address::UTXOChain;
use wormhole_bridge_client::{WormholeBridgeClient, WormholeBridgeClientBuilder};

use crate::{
    config::{self, Network},
    startup,
};

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

pub fn to_chain(config: &config::Config, chain: ChainKind) -> Option<UTXOChain> {
    match (&config.near.network, chain) {
        (Network::Mainnet, ChainKind::Btc) => Some(UTXOChain::BitcoinMainnet),
        (Network::Mainnet, ChainKind::Zcash) => Some(UTXOChain::ZcashMainnet),
        (Network::Testnet, ChainKind::Btc) => Some(UTXOChain::BitcoinTestnet),
        (Network::Testnet, ChainKind::Zcash) => Some(UTXOChain::ZcashTestnet),
        _ => None,
    }
}

fn build_utxo_bridges(
    config: &config::Config,
    near_signer: &InMemorySigner,
) -> HashMap<UTXOChain, UTXOChainAccounts> {
    let mut utxo_bridges = HashMap::new();

    for (chain, connector, token) in [
        (
            ChainKind::Btc,
            config.near.btc_connector.as_ref(),
            config.near.btc.as_ref(),
        ),
        (
            ChainKind::Zcash,
            config.near.zcash_connector.as_ref(),
            config.near.zcash.as_ref(),
        ),
    ] {
        let Some(chain) = to_chain(config, chain) else {
            warn!("Skipping UTXO bridge for unsupported chain: {chain:?}");
            continue;
        };
        utxo_bridges.insert(
            chain,
            UTXOChainAccounts {
                utxo_chain_connector: connector.map(std::string::ToString::to_string),
                utxo_chain_token: token.map(std::string::ToString::to_string),
                satoshi_relayer: Some(near_signer.account_id.to_string()),
            },
        );
    }

    utxo_bridges
}

fn build_near_bridge_client(
    config: &config::Config,
    near_signer: &InMemorySigner,
) -> Result<near_bridge_client::NearBridgeClient> {
    NearBridgeClientBuilder::default()
        .endpoint(Some(config.near.rpc_url.clone()))
        .private_key(Some(near_signer.secret_key.to_string()))
        .signer(Some(near_signer.account_id.to_string()))
        .omni_bridge_id(Some(config.near.omni_bridge_id.to_string()))
        .utxo_bridges(build_utxo_bridges(config, near_signer))
        .build()
        .context("Failed to build NearBridgeClient")
}

fn build_evm_bridge_client(
    config: &config::Config,
    chain_kind: ChainKind,
) -> Result<Option<EvmBridgeClient>> {
    let evm = match chain_kind {
        ChainKind::Eth => &config.eth,
        ChainKind::Base => &config.base,
        ChainKind::Arb => &config.arb,
        ChainKind::Bnb => &config.bnb,
        ChainKind::Near | ChainKind::Sol | ChainKind::Btc | ChainKind::Zcash => {
            unreachable!("Function `build_evm_bridge_client` supports only EVM chains")
        }
    };

    evm.as_ref()
        .map(|evm| {
            EvmBridgeClientBuilder::default()
                .endpoint(Some(evm.rpc_http_url.clone()))
                .chain_id(Some(evm.chain_id))
                .private_key(Some(crate::config::get_private_key(chain_kind, None)))
                .omni_bridge_address(Some(evm.omni_bridge_address.to_string()))
                .wormhole_core_address(evm.wormhole_address.map(|address| address.to_string()))
                .build()
                .context(format!("Failed to build EvmBridgeClient ({chain_kind:?})"))
        })
        .transpose()
}

fn build_solana_bridge_client(config: &config::Config) -> Result<Option<SolanaBridgeClient>> {
    config
        .solana
        .as_ref()
        .map(|solana| {
            SolanaBridgeClientBuilder::default()
                .client(Some(RpcClient::new(solana.rpc_http_url.clone())))
                .program_id(Some(solana.program_id.parse()?))
                .wormhole_core(Some(solana.wormhole_id.parse()?))
                .wormhole_post_message_shim_program_id(Some(
                    solana.wormhole_post_message_shim_id.parse()?,
                ))
                .wormhole_post_message_shim_event_authority(Some(
                    solana.wormhole_post_message_shim_event_authority.parse()?,
                ))
                .keypair(Some(startup::solana::get_keypair(
                    solana.credentials_path.as_ref(),
                )))
                .build()
                .context("Failed to build SolanaBridgeClient")
        })
        .transpose()
}

fn build_utxo_bridge_client<C: utxo_bridge_client::types::UTXOChain>(
    config: &config::Config,
    chain: UtxoChain,
) -> Result<UTXOBridgeClient<C>> {
    let utxo = match chain {
        UtxoChain::Btc => &config.btc,
        UtxoChain::Zcash => &config.zcash,
    };

    utxo.as_ref()
        .map(|utxo| UTXOBridgeClient::new(utxo.rpc_http_url.clone(), AuthOptions::None))
        .context("Failed to create UtxoBridgeClient")
}

fn build_wormhole_bridge_client(config: &config::Config) -> Result<WormholeBridgeClient> {
    WormholeBridgeClientBuilder::default()
        .endpoint(Some(config.wormhole.api_url.clone()))
        .build()
        .context("Failed to build WormholeBridgeClient")
}

fn build_eth_light_client(config: &config::Config) -> Result<Option<EthLightClient>> {
    config
        .eth
        .as_ref()
        .map(|eth| {
            EthLightClientBuilder::default()
                .endpoint(Some(config.near.rpc_url.clone()))
                .eth_light_client_id(
                    eth.light_client
                        .as_ref()
                        .map(std::string::ToString::to_string),
                )
                .build()
                .context("Failed to build EthLightClient")
        })
        .transpose()
}

pub fn build_omni_connector(
    config: &config::Config,
    near_signer: &InMemorySigner,
) -> Result<OmniConnector> {
    info!("Building Omni connector");

    let near_bridge_client = build_near_bridge_client(config, near_signer)?;
    let eth_bridge_client = build_evm_bridge_client(config, ChainKind::Eth)?;
    let base_bridge_client = build_evm_bridge_client(config, ChainKind::Base)?;
    let arb_bridge_client = build_evm_bridge_client(config, ChainKind::Arb)?;
    let bnb_bridge_client = build_evm_bridge_client(config, ChainKind::Bnb)?;
    let solana_bridge_client = build_solana_bridge_client(config)?;
    let btc_bridge_client = build_utxo_bridge_client(config, UtxoChain::Btc)?;
    let zcash_bridge_client = build_utxo_bridge_client(config, UtxoChain::Zcash)?;
    let wormhole_bridge_client = build_wormhole_bridge_client(config)?;
    let eth_light_client = build_eth_light_client(config)?;

    let omni_connector = OmniConnectorBuilder::default()
        .near_bridge_client(Some(near_bridge_client))
        .eth_bridge_client(eth_bridge_client)
        .base_bridge_client(base_bridge_client)
        .arb_bridge_client(arb_bridge_client)
        .bnb_bridge_client(bnb_bridge_client)
        .solana_bridge_client(solana_bridge_client)
        .wormhole_bridge_client(Some(wormhole_bridge_client))
        .btc_bridge_client(Some(btc_bridge_client))
        .zcash_bridge_client(Some(zcash_bridge_client))
        .eth_light_client(eth_light_client)
        .build()
        .context("Failed to build OmniConnector")?;

    Ok(omni_connector)
}
