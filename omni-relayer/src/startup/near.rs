use anyhow::{Context, Result};
use futures::StreamExt;
use log::info;
use redis::AsyncCommands;
use tokio::sync::mpsc;

use near_crypto::InMemorySigner;
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{LakeConfig, LakeConfigBuilder};
use omni_types::near_events::Nep141LockerEvent;

use crate::utils;

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

async fn create_lake_config(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: &JsonRpcClient,
) -> Result<LakeConfig> {
    let start_block_height: u64 = match redis_connection.get("near_last_processed_block").await {
        Ok(block_height) => block_height,
        Err(_) => utils::near::get_final_block(jsonrpc_client).await?,
    };

    info!("NEAR Lake will start from block: {}", start_block_height);

    LakeConfigBuilder::default()
        .testnet()
        .start_block_height(start_block_height)
        .build()
        .context("Failed to build LakeConfig")
}

pub async fn start_indexer(
    config: crate::Config,
    redis_client: redis::Client,
    jsonrpc_client: JsonRpcClient,
    sign_tx: mpsc::UnboundedSender<Nep141LockerEvent>,
    finalize_transfer_tx: mpsc::UnboundedSender<Nep141LockerEvent>,
) -> Result<()> {
    info!("Starting NEAR indexer");

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let lake_config = create_lake_config(&mut redis_connection, &jsonrpc_client).await?;
    let (_, stream) = near_lake_framework::streamer(lake_config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    stream
        .map(move |streamer_message| {
            let mut redis_connection = redis_connection.clone();
            let config = config.clone();
            let sign_tx = sign_tx.clone();
            let finalize_transfer_tx = finalize_transfer_tx.clone();

            async move {
                if let Err(err) = redis_connection
                    .set::<&str, u64, ()>(
                        "near_last_processed_block",
                        streamer_message.block.header.height,
                    )
                    .await
                {
                    log::warn!(
                        "Failed to update last near processed block in redis-db: {}",
                        err
                    );
                }

                utils::near::handle_streamer_message(
                    &config,
                    &streamer_message,
                    &sign_tx,
                    &finalize_transfer_tx,
                );
            }
        })
        .buffer_unordered(10)
        .for_each(|()| async {})
        .await;

    Ok(())
}
