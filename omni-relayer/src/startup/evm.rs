use anyhow::{Context, Result};
use log::{info, warn};
use omni_types::ChainKind;
use reqwest::Client;
use tokio_stream::StreamExt;

use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder, RootProvider, WsConnect},
    rpc::types::{Filter, Log},
    sol_types::SolEvent,
    transports::http::Http,
};
use ethereum_types::H256;

use crate::{config, utils, workers::near::FinTransfer};

fn extract_evm_config(evm: config::Evm) -> (String, String, Address, u64, i64) {
    (
        evm.rpc_http_url,
        evm.rpc_ws_url,
        evm.bridge_token_factory_address,
        evm.block_processing_batch_size,
        evm.expected_finalization_time,
    )
}

pub async fn start_indexer(
    config: config::Config,
    redis_client: redis::Client,
    chain_kind: ChainKind,
    start_block: Option<u64>,
) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let (
        rpc_http_url,
        rpc_ws_url,
        bridge_token_factory_address,
        block_processing_batch_size,
        expected_finalization_time,
    ) = match chain_kind {
        ChainKind::Eth => extract_evm_config(config.eth.context("Failed to get Eth config")?),
        ChainKind::Base => extract_evm_config(config.base.context("Failed to get Base config")?),
        ChainKind::Arb => extract_evm_config(config.arb.context("Failed to get Arb config")?),
        _ => anyhow::bail!("Unsupported chain kind: {chain_kind:?}"),
    };

    let http_provider = ProviderBuilder::new().on_http(rpc_http_url.parse().context(format!(
        "Failed to parse {chain_kind:?} rpc provider as url",
    ))?);

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(rpc_ws_url))
        .await
        .context(format!("Failed to initialize {chain_kind:?} WS provider"))?;

    let last_processed_block_key = utils::redis::get_last_processed_key(chain_kind);
    let latest_block = http_provider.get_block_number().await?;
    let from_block = match start_block {
        Some(block) => block,
        None => {
            utils::redis::get_last_processed(&mut redis_connection, &last_processed_block_key)
                .await
                .unwrap_or(latest_block)
                + 1
        }
    };

    let filter = Filter::new()
        .address(bridge_token_factory_address)
        .event_signature(
            [
                utils::evm::InitTransfer::SIGNATURE_HASH,
                utils::evm::FinTransfer::SIGNATURE_HASH,
            ]
            .to_vec(),
        );

    for current_block in
        (from_block..latest_block).step_by(usize::try_from(block_processing_batch_size)?)
    {
        let logs = http_provider
            .get_logs(
                &filter
                    .clone()
                    .from_block(current_block)
                    .to_block((current_block + block_processing_batch_size).min(latest_block)),
            )
            .await?;

        for log in logs {
            process_log(
                chain_kind,
                &mut redis_connection,
                &http_provider,
                log,
                expected_finalization_time,
            )
            .await;
        }
    }

    info!(
        "All historical logs processed, starting {:?} WS subscription",
        chain_kind
    );

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        process_log(
            chain_kind,
            &mut redis_connection,
            &http_provider,
            log,
            expected_finalization_time,
        )
        .await;
    }

    Ok(())
}

async fn process_log(
    chain_kind: ChainKind,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    http_provider: &RootProvider<Http<Client>>,
    log: Log,
    expected_finalization_time: i64,
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
                tx_logs: tx_logs.map(Box::new),
                creation_timestamp: chrono::Utc::now().timestamp(),
                last_update_timestamp: None,
                expected_finalization_time,
            },
        )
        .await;
    } else if log.log_decode::<utils::evm::FinTransfer>().is_ok() {
        utils::redis::add_event(
            redis_connection,
            utils::redis::FINALIZED_TRANSFERS,
            tx_hash.to_string(),
            FinTransfer::Evm {
                chain_kind,
                block_number,
                log,
                tx_logs: tx_logs.map(Box::new),
                creation_timestamp: chrono::Utc::now().timestamp(),
                expected_finalization_time,
            },
        )
        .await;
    }

    utils::redis::update_last_processed(
        redis_connection,
        &utils::redis::get_last_processed_key(chain_kind),
        block_number,
    )
    .await;
}
