use anyhow::{Context, Result};
use log::{info, warn};
use omni_types::ChainKind;
use reqwest::Client;
use tokio_stream::StreamExt;

use alloy::{
    providers::{Provider, ProviderBuilder, RootProvider, WsConnect},
    rpc::types::{Filter, Log},
    sol_types::SolEvent,
    transports::http::Http,
};
use ethereum_types::H256;

use crate::{config, utils, workers::near::FinTransfer};

pub async fn start_eth_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let http_provider = ProviderBuilder::new().on_http(
        config
            .eth
            .rpc_http_url
            .parse()
            .context("Failed to parse ETH rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(config.eth.rpc_ws_url.clone()))
        .await
        .context("Failed to initialize ETH WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block = utils::redis::get_last_processed_block(
        &mut redis_connection,
        utils::redis::ETH_LAST_PROCESSED_BLOCK,
    )
    .await
    .map_or_else(
        || latest_block.saturating_sub(config.eth.block_processing_batch_size),
        |block| block,
    );

    let filter = Filter::new()
        .address(config.eth.bridge_token_factory_address)
        .event_signature(
            [
                utils::evm::InitTransfer::SIGNATURE_HASH,
                utils::evm::FinTransfer::SIGNATURE_HASH,
            ]
            .to_vec(),
        );

    for current_block in
        (from_block..latest_block).step_by(config.eth.block_processing_batch_size as usize)
    {
        let logs = http_provider
            .get_logs(&filter.clone().from_block(current_block).to_block(
                (current_block + config.eth.block_processing_batch_size).min(latest_block),
            ))
            .await?;

        for log in logs {
            process_log(ChainKind::Eth, &mut redis_connection, &http_provider, log).await;
        }
    }

    info!("All historical logs processed, starting ETH WS subscription");

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        process_log(ChainKind::Eth, &mut redis_connection, &http_provider, log).await;
    }

    Ok(())
}

pub async fn start_base_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let http_provider = ProviderBuilder::new().on_http(
        config
            .base
            .rpc_http_url
            .parse()
            .context("Failed to parse BASE rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(config.base.rpc_ws_url.clone()))
        .await
        .context("Failed to initialize BASE WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block = utils::redis::get_last_processed_block(
        &mut redis_connection,
        utils::redis::BASE_LAST_PROCESSED_BLOCK,
    )
    .await
    .map_or_else(
        || latest_block.saturating_sub(config.base.block_processing_batch_size),
        |block| block,
    );

    let filter = Filter::new()
        .address(config.base.bridge_token_factory_address)
        .event_signature(
            [
                utils::evm::InitTransfer::SIGNATURE_HASH,
                utils::evm::FinTransfer::SIGNATURE_HASH,
            ]
            .to_vec(),
        );

    for current_block in
        (from_block..latest_block).step_by(config.base.block_processing_batch_size as usize)
    {
        let logs = http_provider
            .get_logs(&filter.clone().from_block(current_block).to_block(
                (current_block + config.base.block_processing_batch_size).min(latest_block),
            ))
            .await?;

        for log in logs {
            process_log(ChainKind::Base, &mut redis_connection, &http_provider, log).await;
        }
    }

    info!("All historical logs processed, starting BASE WS subscription");

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        process_log(ChainKind::Base, &mut redis_connection, &http_provider, log).await;
    }

    Ok(())
}

pub async fn start_arb_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let http_provider = ProviderBuilder::new().on_http(
        config
            .arb
            .rpc_http_url
            .parse()
            .context("Failed to parse ARB rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(config.arb.rpc_ws_url.clone()))
        .await
        .context("Failed to initialize ARB WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block = utils::redis::get_last_processed_block(
        &mut redis_connection,
        utils::redis::ARB_LAST_PROCESSED_BLOCK,
    )
    .await
    .map_or_else(
        || latest_block.saturating_sub(config.arb.block_processing_batch_size),
        |block| block,
    );

    let filter = Filter::new()
        .address(config.arb.bridge_token_factory_address)
        .event_signature(
            [
                utils::evm::InitTransfer::SIGNATURE_HASH,
                utils::evm::FinTransfer::SIGNATURE_HASH,
            ]
            .to_vec(),
        );

    for current_block in
        (from_block..latest_block).step_by(config.arb.block_processing_batch_size as usize)
    {
        let logs = http_provider
            .get_logs(&filter.clone().from_block(current_block).to_block(
                (current_block + config.arb.block_processing_batch_size).min(latest_block),
            ))
            .await?;

        for log in logs {
            process_log(ChainKind::Arb, &mut redis_connection, &http_provider, log).await;
        }
    }

    info!("All historical logs processed, starting ARB WS subscription");

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        process_log(ChainKind::Arb, &mut redis_connection, &http_provider, log).await;
    }

    Ok(())
}

async fn process_log(
    chain_kind: ChainKind,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    http_provider: &RootProvider<Http<Client>>,
    log: Log,
) {
    let Some(tx_hash) = log.transaction_hash else {
        warn!("No transaction hash in log: {:?}", log);
        return;
    };

    let Ok(tx_logs) = http_provider.get_transaction_receipt(tx_hash).await else {
        warn!("Failed to get transaction receipt for tx: {:?}", tx_hash);
        return;
    };

    let tx_hash = H256::from_slice(tx_hash.as_slice());

    let Some(block_number) = log.block_number else {
        warn!("No block number in log: {:?}", log);
        return;
    };

    if log.log_decode::<utils::evm::InitTransfer>().is_ok() {
        utils::redis::add_event(
            redis_connection,
            utils::redis::EVM_INIT_TRANSFER_EVENTS,
            tx_hash.to_string(),
            crate::workers::evm::InitTransferWithTimestamp {
                chain_kind,
                block_number,
                log,
                tx_logs,
                creation_timestamp: chrono::Utc::now().timestamp(),
                last_update_timestamp: None,
            },
        )
        .await;
    } else if log.log_decode::<utils::evm::FinTransfer>().is_ok() {
        utils::redis::add_event(
            redis_connection,
            utils::redis::FINALIZED_TRANSFERS,
            tx_hash.to_string(),
            FinTransfer {
                chain_kind,
                block_number,
                log,
                tx_logs,
            },
        )
        .await;
    }

    utils::redis::update_last_processed_block(
        redis_connection,
        utils::redis::ETH_LAST_PROCESSED_BLOCK,
        block_number,
    )
    .await;
}
