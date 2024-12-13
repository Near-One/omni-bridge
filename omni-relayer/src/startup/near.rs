use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use futures::StreamExt;
use log::info;

use near_crypto::{InMemorySigner, SecretKey};
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{LakeConfig, LakeConfigBuilder};
use near_primitives::types::AccountId;
use omni_types::ChainKind;

use crate::{config, utils};

fn get_account_id(file: Option<&String>) -> Result<AccountId> {
    if let Some(file) = file {
        if let Some(file_stem) = Path::new(file).file_stem().and_then(|s| s.to_str()) {
            if let Ok(account_id) = file_stem.parse::<AccountId>() {
                info!("Retrieved account_id from filename: {}", account_id);
                return Ok(account_id);
            }
        }
    }

    let account_id = std::env::var("NEAR_ACCOUNT_ID")
        .context("Failed to get `NEAR_ACCOUNT_ID` environment variable")?;

    info!("Retrieved account_id from env: {}", account_id);

    account_id
        .parse()
        .context("Failed to parse `NEAR_ACCOUNT_ID`")
}

fn get_private_key(file: Option<String>) -> Result<SecretKey> {
    if let Some(file) = file {
        if let Ok(file_content) = std::fs::read_to_string(file) {
            if let Ok(key_data) = serde_json::from_str::<HashMap<String, String>>(&file_content) {
                if let Some(private_key_str) = key_data.get("private_key") {
                    if let Ok(private_key) = private_key_str.parse::<SecretKey>() {
                        info!("Retrieved private key from file");
                        return Ok(private_key);
                    }
                }
            }
        }
    }

    let private_key_str = std::env::var("NEAR_PRIVATE_KEY")
        .context("Failed to get `NEAR_PRIVATE_KEY` environment variable")?;

    info!("Retrieved private key from env");

    private_key_str
        .parse()
        .context("Failed to parse private key")
}

pub fn create_signer(file: Option<String>) -> Result<InMemorySigner> {
    info!("Creating NEAR signer");

    let account_id = get_account_id(file.as_ref())?;
    let private_key = get_private_key(file)?;

    Ok(InMemorySigner::from_secret_key(account_id, private_key))
}

async fn create_lake_config(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: &JsonRpcClient,
) -> Result<LakeConfig> {
    let start_block_height = match utils::redis::get_last_processed_block(
        redis_connection,
        &utils::redis::get_last_processed_block_key(ChainKind::Near).await,
    )
    .await
    {
        Some(block_height) => block_height,
        None => utils::near::get_final_block(jsonrpc_client).await?,
    };

    info!("NEAR Lake will start from block: {}", start_block_height);

    let lake_config = LakeConfigBuilder::default().start_block_height(start_block_height);

    match config.near.network {
        config::Network::Testnet => lake_config
            .testnet()
            .build()
            .context("Failed to build testnet LakeConfig"),
        config::Network::Mainnet => lake_config
            .mainnet()
            .build()
            .context("Failed to build mainnet LakeConfig"),
    }
}

pub async fn start_indexer(
    config: config::Config,
    redis_client: redis::Client,
    jsonrpc_client: JsonRpcClient,
) -> Result<()> {
    info!("Starting NEAR indexer");

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let lake_config = create_lake_config(&config, &mut redis_connection, &jsonrpc_client).await?;
    let (_, stream) = near_lake_framework::streamer(lake_config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    stream
        .map(move |streamer_message| {
            let config = config.clone();
            let mut redis_connection = redis_connection.clone();

            async move {
                utils::near::handle_streamer_message(
                    &config,
                    &mut redis_connection,
                    &streamer_message,
                )
                .await;

                utils::redis::update_last_processed_block(
                    &mut redis_connection,
                    &utils::redis::get_last_processed_block_key(ChainKind::Near).await,
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
