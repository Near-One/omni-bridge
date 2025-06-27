use std::path::Path;

use anyhow::{Context, Result};
use futures::StreamExt;
use log::info;

use near_crypto::{InMemorySigner, Signer};
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{LakeConfig, LakeConfigBuilder};
use omni_types::ChainKind;

use crate::{config, utils};

pub fn get_signer(
    file: Option<&String>,
    near_signer_type: config::NearSignerType,
) -> Result<InMemorySigner> {
    info!("Creating NEAR signer");

    if let Some(file) = file {
        info!("Using NEAR credentials file: {file}");
        if let Ok(Signer::InMemory(signer)) = InMemorySigner::from_file(Path::new(file)) {
            return Ok(signer);
        }
    }

    info!("Retrieving NEAR credentials from env");

    let account_id_env = match near_signer_type {
        config::NearSignerType::Omni => "NEAR_OMNI_ACCOUNT_ID",
        config::NearSignerType::Fast => "NEAR_FAST_ACCOUNT_ID",
    };

    let account_id = std::env::var(account_id_env)
        .context(format!(
            "Failed to get `{account_id_env}` environment variable"
        ))?
        .parse()
        .context(format!("Failed to parse `{account_id_env}`"))?;

    let private_key = config::get_private_key(ChainKind::Near, Some(near_signer_type))
        .parse()
        .context("Failed to parse private key")?;

    if let Signer::InMemory(signer) = InMemorySigner::from_secret_key(account_id, private_key) {
        Ok(signer)
    } else {
        anyhow::bail!("Failed to create NEAR signer")
    }
}

async fn create_lake_config(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: &JsonRpcClient,
    start_block: Option<u64>,
) -> Result<LakeConfig> {
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

    info!("NEAR Lake will start from block: {start_block_height}");

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
    start_block: Option<u64>,
) -> Result<()> {
    info!("Starting NEAR indexer");

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let lake_config =
        create_lake_config(&config, &mut redis_connection, &jsonrpc_client, start_block).await?;
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

                utils::redis::update_last_processed(
                    &mut redis_connection,
                    &utils::redis::get_last_processed_key(ChainKind::Near),
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
