use anyhow::{Context, Result};
use redis::AsyncCommands;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use alloy::{
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::{Filter, Log},
};

use crate::{defaults, utils};

pub async fn start_indexer(
    config: crate::Config,
    redis_client: redis::Client,
    finalize_withdraw_tx: mpsc::UnboundedSender<Log>,
) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let http_provider = ProviderBuilder::new().on_http(
        defaults::ETH_RPC_MAINNET
            .parse()
            .context("Failed to parse ETH rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(defaults::ETH_WS_MAINNET))
        .await
        .context("Failed to initialize WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block = match redis_connection.get("eth_last_processed_block").await {
        Ok(block) => block,
        Err(_) => latest_block.saturating_sub(10_000),
    };

    let filter = Filter::new()
        .address(config.bridge_token_factory_address_mainnet)
        .event("Withdraw(string,address,uint256,string,address)");

    for current_block in (from_block..latest_block).step_by(10_000) {
        let logs = http_provider
            .get_logs(
                &filter
                    .clone()
                    .from_block(current_block)
                    .to_block(current_block + 10_000),
            )
            .await?;
        for log in logs {
            process_log(&mut redis_connection, &log).await;
            if let Err(err) = finalize_withdraw_tx.send(log) {
                log::warn!("Failed to send log: {}", err);
            }
        }
    }

    let logs = http_provider
        .get_logs(&filter.clone().from_block(from_block).to_block(latest_block))
        .await?;
    for log in logs {
        process_log(&mut redis_connection, &log).await;

        if let Err(err) = finalize_withdraw_tx.send(log) {
            log::warn!("Failed to send log: {}", err);
        }
    }

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        process_log(&mut redis_connection, &log).await;

        if let Err(err) = finalize_withdraw_tx.send(log) {
            log::warn!("Failed to send log: {}", err);
        }
    }

    Ok(())
}

async fn process_log(redis_connection: &mut redis::aio::MultiplexedConnection, log: &Log) {
    if let Some(block_height) = log.block_number {
        utils::redis::update_last_processed_block(
            redis_connection,
            "eth_last_processed_block",
            block_height,
        )
        .await;
    }

    utils::redis::add_event(redis_connection, "eth_withdraw_events", log.clone()).await;
}
