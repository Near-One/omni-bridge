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

pub async fn start_indexer(
    config: config::Config,
    redis_client: redis::Client,
    chain_kind: ChainKind,
) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let (
        rpc_http_url,
        rpc_ws_url,
        bridge_token_factory_address,
        last_processed_block_key,
        block_processing_batch_size,
    ) = match chain_kind {
        ChainKind::Eth => (
            config.eth.rpc_http_url.clone(),
            config.eth.rpc_ws_url.clone(),
            config.eth.bridge_token_factory_address,
            utils::redis::ETH_LAST_PROCESSED_BLOCK,
            config.eth.block_processing_batch_size,
        ),
        ChainKind::Base => (
            config.base.rpc_http_url.clone(),
            config.base.rpc_ws_url.clone(),
            config.base.bridge_token_factory_address,
            utils::redis::BASE_LAST_PROCESSED_BLOCK,
            config.base.block_processing_batch_size,
        ),
        ChainKind::Arb => (
            config.arb.rpc_http_url.clone(),
            config.arb.rpc_ws_url.clone(),
            config.arb.bridge_token_factory_address,
            utils::redis::ARB_LAST_PROCESSED_BLOCK,
            config.arb.block_processing_batch_size,
        ),
        _ => anyhow::bail!("Unsupported chain kind: {:?}", chain_kind),
    };

    let http_provider = ProviderBuilder::new().on_http(
        rpc_http_url
            .parse()
            .context("Failed to parse ETH rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(rpc_ws_url))
        .await
        .context("Failed to initialize ETH WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block =
        utils::redis::get_last_processed_block(&mut redis_connection, last_processed_block_key)
            .await
            .map_or_else(
                || latest_block.saturating_sub(block_processing_batch_size),
                |block| block,
            );

    let filter = Filter::new()
        .address(bridge_token_factory_address)
        .event_signature(
            [
                utils::evm::InitTransfer::SIGNATURE_HASH,
                utils::evm::FinTransfer::SIGNATURE_HASH,
            ]
            .to_vec(),
        );

    for current_block in (from_block..latest_block).step_by(block_processing_batch_size as usize) {
        let logs = http_provider
            .get_logs(
                &filter
                    .clone()
                    .from_block(current_block)
                    .to_block((current_block + block_processing_batch_size).min(latest_block)),
            )
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
