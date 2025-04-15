use alloy::{
    primitives::Address,
    providers::{
        fillers::{BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller},
        Identity, Provider, ProviderBuilder, RootProvider, WsConnect,
    },
    rpc::types::{Filter, Log},
    sol_types::SolEvent,
};
use anyhow::{Context, Result};
use ethereum_types::H256;
use log::{error, info, warn};
use omni_types::ChainKind;
use reqwest::Url;
use tokio_stream::StreamExt;

use crate::{
    config, utils,
    workers::{DeployToken, FinTransfer},
};

pub type EvmProvider = FillProvider<
    JoinFill<
        Identity,
        JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
    >,
    RootProvider,
>;

fn hide_api_key<E: ToString>(err: &E) -> String {
    let env_key = "INFURA_API_KEY";
    let api_key = std::env::var(env_key).unwrap_or_default();
    err.to_string().replace(&api_key, env_key)
}

fn extract_evm_config(evm: config::Evm) -> Result<(Url, String, Address, u64, i64)> {
    Ok((
        evm.rpc_http_url
            .parse()
            .context("Failed to parse EVM rpc provider as url")?,
        evm.rpc_ws_url,
        evm.bridge_token_factory_address,
        evm.block_processing_batch_size,
        evm.expected_finalization_time,
    ))
}

pub async fn start_indexer(
    config: config::Config,
    redis_client: redis::Client,
    chain_kind: ChainKind,
    mut start_block: Option<u64>,
) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let (
        rpc_http_url,
        rpc_ws_url,
        bridge_token_factory_address,
        block_processing_batch_size,
        expected_finalization_time,
    ) = match chain_kind {
        ChainKind::Eth => extract_evm_config(config.eth.context("Failed to get Eth config")?)?,
        ChainKind::Base => extract_evm_config(config.base.context("Failed to get Base config")?)?,
        ChainKind::Arb => extract_evm_config(config.arb.context("Failed to get Arb config")?)?,
        _ => anyhow::bail!("Unsupported chain kind: {chain_kind:?}"),
    };

    let filter = Filter::new()
        .address(bridge_token_factory_address)
        .event_signature(
            [
                utils::evm::InitTransfer::SIGNATURE_HASH,
                utils::evm::FinTransfer::SIGNATURE_HASH,
                utils::evm::DeployToken::SIGNATURE_HASH,
            ]
            .to_vec(),
        );

    loop {
        let http_provider = ProviderBuilder::new().on_http(rpc_http_url.clone());

        crate::skip_fail!(
            process_recent_blocks(
                &mut redis_connection,
                &http_provider,
                &filter,
                chain_kind,
                start_block,
                block_processing_batch_size,
                expected_finalization_time,
            )
            .await
            .map_err(|err| hide_api_key(&err)),
            format!("Failed to process recent blocks for {chain_kind:?} indexer"),
            5
        );

        info!("All historical logs processed, starting {chain_kind:?} WS subscription");

        let ws_provider = crate::skip_fail!(
            ProviderBuilder::new()
                .on_ws(WsConnect::new(&rpc_ws_url))
                .await
                .map_err(|err| hide_api_key(&err)),
            format!("{chain_kind:?} WebSocket connection failed"),
            5
        );

        let mut stream = crate::skip_fail!(
            ws_provider
                .subscribe_logs(&filter)
                .await
                .map_err(|err| hide_api_key(&err)),
            format!("{chain_kind:?} WebSocket subscription failed"),
            5
        )
        .into_stream();

        info!("Subscribed to {chain_kind:?} logs");

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

        error!("{chain_kind:?} WebSocket stream closed unexpectedly, reconnecting...");
        start_block = None;

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn process_recent_blocks(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    http_provider: &EvmProvider,
    filter: &Filter,
    chain_kind: ChainKind,
    start_block: Option<u64>,
    block_processing_batch_size: u64,
    expected_finalization_time: i64,
) -> Result<()> {
    let last_processed_block_key = utils::redis::get_last_processed_key(chain_kind);
    let latest_block = http_provider.get_block_number().await?;
    let from_block = match start_block {
        Some(block) => block,
        None => {
            if let Some(block) = utils::redis::get_last_processed::<&str, u64>(
                redis_connection,
                &last_processed_block_key,
            )
            .await
            {
                block + 1
            } else {
                utils::redis::update_last_processed(
                    redis_connection,
                    &last_processed_block_key,
                    latest_block + 1,
                )
                .await;
                latest_block + 1
            }
        }
    };

    info!("{chain_kind:?} indexer will start from block: {from_block}");

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
                redis_connection,
                http_provider,
                log,
                expected_finalization_time,
            )
            .await;
        }
    }

    Ok(())
}

async fn process_log(
    chain_kind: ChainKind,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    http_provider: &EvmProvider,
    log: Log,
    expected_finalization_time: i64,
) {
    let Some(tx_hash) = log.transaction_hash else {
        warn!("No transaction hash in log: {log:?}");
        return;
    };

    let tx_hash = H256::from_slice(tx_hash.as_slice());

    let Some(block_number) = log.block_number else {
        warn!("No block number in log: {log:?}");
        return;
    };

    let timestamp = http_provider
        .get_block(alloy::eips::BlockId::Number(
            alloy::eips::BlockNumberOrTag::Number(block_number),
        ))
        .await
        .ok()
        .flatten()
        .and_then(|block| i64::try_from(block.header.timestamp).ok())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    let topic = log.topic0();

    if let Ok(init_log) = log.log_decode::<utils::evm::InitTransfer>() {
        info!("Received InitTransfer on {chain_kind:?} ({tx_hash:?})");

        let log = utils::evm::InitTransfer {
            sender: init_log.inner.sender,
            tokenAddress: init_log.inner.tokenAddress,
            originNonce: init_log.inner.originNonce,
            amount: init_log.inner.amount,
            fee: init_log.inner.fee,
            nativeFee: init_log.inner.nativeFee,
            recipient: init_log.inner.recipient.clone(),
            message: init_log.inner.message.clone(),
        };

        utils::redis::add_event(
            redis_connection,
            utils::redis::EVENTS,
            tx_hash.to_string(),
            crate::workers::Transfer::Evm {
                chain_kind,
                block_number,
                tx_hash,
                log,
                creation_timestamp: timestamp,
                last_update_timestamp: None,
                expected_finalization_time,
            },
        )
        .await;
    } else if let Ok(fin_log) = log.log_decode::<utils::evm::FinTransfer>() {
        info!("Received FinTransfer on {chain_kind:?} ({tx_hash:?})");

        let Some(&topic) = topic else {
            warn!("Topic is empty for log: {log:?}");
            return;
        };

        let Ok(origin_chain) = ChainKind::try_from(fin_log.inner.originChain) else {
            warn!("Failed to parse origin chain from log: {log:?}");
            return;
        };

        utils::redis::add_event(
            redis_connection,
            utils::redis::EVENTS,
            tx_hash.to_string(),
            FinTransfer::Evm {
                chain_kind,
                block_number,
                tx_hash,
                topic,
                origin_chain,
                origin_nonce: fin_log.inner.originNonce,
                creation_timestamp: timestamp,
                expected_finalization_time,
            },
        )
        .await;
    } else if log.log_decode::<utils::evm::DeployToken>().is_ok() {
        info!("Received DeployToken on {chain_kind:?} ({tx_hash:?})");

        let Some(&topic) = topic else {
            warn!("Topic is empty for log: {log:?}");
            return;
        };

        utils::redis::add_event(
            redis_connection,
            utils::redis::EVENTS,
            tx_hash.to_string(),
            DeployToken::Evm {
                chain_kind,
                block_number,
                tx_hash,
                topic,
                creation_timestamp: timestamp,
                expected_finalization_time,
            },
        )
        .await;
    } else {
        warn!("Received unknown log on {chain_kind:?}: {log:?}");
    }

    utils::redis::update_last_processed(
        redis_connection,
        &utils::redis::get_last_processed_key(chain_kind),
        block_number,
    )
    .await;
}
