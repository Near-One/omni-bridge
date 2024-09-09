use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;
use log::info;

use near_crypto::InMemorySigner;
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{LakeConfig, LakeConfigBuilder};
use nep141_connector::Nep141Connector;

use crate::{defaults, utils};

pub fn create_signer() -> Result<InMemorySigner> {
    info!("Creating NEAR signer");

    let account_id = std::env::var("NEAR_ACCOUNT_ID")
        .context("Failed to get `NEAR_ACCOUNT_ID` env variable")?
        .parse()?;

    let private_key = std::env::var("NEAR_PRIVATE_KEY")
        .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?
        .parse()?;

    Ok(InMemorySigner::from_secret_key(account_id, private_key))
}

async fn create_lake_config(client: &JsonRpcClient) -> Result<LakeConfig> {
    let final_block = utils::near::get_final_block(client).await?;
    info!("NEAR Lake will start from block: {}", final_block);

    LakeConfigBuilder::default()
        .testnet()
        .start_block_height(final_block)
        .build()
        .context("Failed to build LakeConfig")
}

pub async fn start_indexer(
    near_signer: InMemorySigner,
    connector: Arc<Nep141Connector>,
) -> Result<()> {
    info!("Starting NEAR indexer");

    let client = JsonRpcClient::connect(defaults::NEAR_RPC_TESTNET);

    let config = create_lake_config(&client).await?;
    let (_, stream) = near_lake_framework::streamer(config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    stream
        .map(|streamer_message| async {
            utils::near::handle_streamer_message(
                &client,
                &near_signer,
                &connector,
                streamer_message,
            );
        })
        .buffer_unordered(10)
        .for_each(|()| async {})
        .await;

    Ok(())
}
