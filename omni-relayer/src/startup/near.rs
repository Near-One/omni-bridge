use std::path::Path;

use anyhow::{Context, Result};
use log::info;

use near_crypto::{InMemorySigner, Signer};
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{Lake, LakeBuilder};
use omni_types::ChainKind;
use tokio::task;

use crate::{config, utils};

pub fn get_signer(file: Option<&String>) -> Result<InMemorySigner> {
    info!("Creating NEAR signer");

    if let Some(file) = file {
        info!("Using NEAR credentials file: {}", file);
        if let Ok(Signer::InMemory(signer)) = InMemorySigner::from_file(Path::new(file)) {
            return Ok(signer);
        }
    }

    info!("Retrieving NEAR credentials from env");

    let account_id = std::env::var("NEAR_ACCOUNT_ID")
        .context("Failed to get `NEAR_ACCOUNT_ID` environment variable")?
        .parse()
        .context("Failed to parse `NEAR_ACCOUNT_ID`")?;

    let private_key = config::get_private_key(ChainKind::Near)
        .parse()
        .context("Failed to parse private key")?;

    if let Signer::InMemory(signer) = InMemorySigner::from_secret_key(account_id, private_key) {
        Ok(signer)
    } else {
        anyhow::bail!("Failed to create NEAR signer")
    }
}

async fn create_lake(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: &JsonRpcClient,
    start_block: Option<u64>,
) -> Result<Lake> {
    let start_block_height = match start_block {
        Some(block) => block,
        None => utils::redis::get_last_processed::<&str, u64>(
            redis_connection,
            &utils::redis::get_last_processed_key(ChainKind::Near),
        )
        .await
        .map_or(
            utils::near::get_final_block(jsonrpc_client).await?,
            |block_height| block_height + 1,
        ),
    };

    info!("NEAR Lake will start from block: {}", start_block_height);

    let lake_config = LakeBuilder::default().start_block_height(start_block_height);

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
    start_block: Option<u64>,
) -> Result<()> {
    info!("Starting NEAR indexer");

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let lake = create_lake(&config, &mut redis_connection, &jsonrpc_client, start_block).await?;
    let run_lake = task::spawn_blocking(move || {
        lake.run(move |block| {
            utils::near::handle_streamer_message(
                config.clone(),
                redis_connection.clone(),
                block.streamer_message().clone(),
            )
        })
    })
    .await?;

    run_lake.context("Failed to run NEAR Lake")
}
