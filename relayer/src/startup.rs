use anyhow::{Context, Result};
use log::info;

use near_crypto::InMemorySigner;
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{LakeConfig, LakeConfigBuilder};

use crate::{defaults, utils};

pub fn create_near_signer() -> Result<InMemorySigner> {
    let account_id = std::env::var("NEAR_ACCOUNT_ID")
        .context("Failed to get `NEAR_ACCOUNT_ID` env variable")?
        .parse()?;

    let private_key = std::env::var("NEAR_PRIVATE_KEY")
        .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?
        .parse()?;

    Ok(InMemorySigner::from_secret_key(account_id, private_key))
}

pub fn build_connector(near_signer: &InMemorySigner) -> Result<nep141_connector::Nep141Connector> {
    nep141_connector::Nep141ConnectorBuilder::default()
        .eth_endpoint(Some(defaults::ETH_RPC_TESTNET.to_string()))
        .eth_chain_id(Some(defaults::ETH_CHAIN_ID_TESTNET))
        .near_endpoint(Some(defaults::NEAR_RPC_TESTNET.to_string()))
        .token_locker_id(Some(defaults::TOKEN_LOCKER_ID_TESTNET.to_string()))
        .bridge_token_factory_address(Some(
            defaults::BRIDGE_TOKEN_FACTORY_ADDRESS_TESTNET.to_string(),
        ))
        .near_light_client_address(Some(
            defaults::NEAR_LIGHT_CLIENT_ETH_ADDRESS_TESTNET.to_string(),
        ))
        .eth_private_key(Some(
            std::env::var("ETH_PRIVATE_KEY")
                .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?,
        ))
        .near_signer(Some(near_signer.account_id.to_string()))
        .near_private_key(Some(near_signer.secret_key.to_string()))
        .build()
        .context("Failed to build Nep141Connector")
}

pub async fn create_lake_config(client: &JsonRpcClient) -> Result<LakeConfig> {
    let final_block = utils::get_final_block(client).await?;
    info!("Starting NEAR Lake from block: {}", final_block);

    LakeConfigBuilder::default()
        .testnet()
        .start_block_height(final_block)
        .build()
        .context("Failed to build LakeConfig")
}
