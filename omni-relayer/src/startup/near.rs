use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use futures::StreamExt;
use log::info;

use near_crypto::InMemorySigner;
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{LakeConfig, LakeConfigBuilder};

use crate::{config, utils};

pub fn create_signer(file: Option<String>) -> Result<InMemorySigner> {
    info!("Creating NEAR signer");

    let account_id = std::env::var("NEAR_ACCOUNT_ID")
        .context("Failed to get `NEAR_ACCOUNT_ID` env variable")?
        .parse()?;

    let private_key = if let Some(file) = file {
        let file_path = Path::new(&file);

        let file_content = std::fs::read_to_string(file_path)
            .context(format!("Failed to read file: {file_path:?}"))?;

        serde_json::from_str::<HashMap<String, String>>(&file_content)
            .context("Failed to parse json from file")?
            .get("private_key")
            .context("Failed to get `private_key` from file")?
            .parse()?
    } else {
        std::env::var("NEAR_PRIVATE_KEY")
            .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?
            .parse()?
    };

    Ok(InMemorySigner::from_secret_key(account_id, private_key))
}

async fn create_lake_config(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: &JsonRpcClient,
) -> Result<LakeConfig> {
    let start_block_height = match utils::redis::get_last_processed_block(
        redis_connection,
        utils::redis::NEAR_LAST_PROCESSED_BLOCK,
    )
    .await
    {
        Some(block_height) => block_height,
        None => utils::near::get_final_block(jsonrpc_client).await?,
    };

    info!("NEAR Lake will start from block: {}", start_block_height);

    LakeConfigBuilder::default()
        .testnet()
        .start_block_height(start_block_height)
        .build()
        .context("Failed to build LakeConfig")
}

pub async fn start_indexer(
    config: config::Config,
    redis_client: redis::Client,
    jsonrpc_client: JsonRpcClient,
) -> Result<()> {
    info!("Starting NEAR indexer");

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let lake_config = create_lake_config(&mut redis_connection, &jsonrpc_client).await?;
    let (_, stream) = near_lake_framework::streamer(lake_config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    stream
        .map(move |streamer_message| {
            let config = config.clone();
            let mut redis_connection = redis_connection.clone();
            let jsonrpc_client = jsonrpc_client.clone();

            async move {
                utils::near::handle_streamer_message(
                    &config,
                    &mut redis_connection,
                    &jsonrpc_client,
                    &streamer_message,
                )
                .await;

                utils::redis::update_last_processed_block(
                    &mut redis_connection,
                    utils::redis::NEAR_LAST_PROCESSED_BLOCK,
                    streamer_message.block.header.height,
                )
                .await;
            }
        })
        .buffer_unordered(10)
        .for_each(|()| async {})
        .await;

    Ok(())
}
