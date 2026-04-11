use anyhow::{Context, Result};
use futures::StreamExt;
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::near_indexer_primitives::{
    IndexerExecutionOutcomeWithReceipt, StreamerMessage,
    views::{ActionView, ReceiptEnumView, ReceiptView},
};
use near_lake_framework::{LakeConfig, LakeConfigBuilder};
use omni_types::{ChainKind, near_events::OmniBridgeEvent};
use tracing::info;

use crate::{config, utils, workers::RetryableEvent};

async fn create_lake_config(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    jsonrpc_client: &JsonRpcClient,
    start_block: Option<u64>,
) -> Result<LakeConfig> {
    let start_block_height = match start_block {
        Some(block) => block,
        None => utils::redis::get_last_processed::<&str, u64>(
            config,
            redis_connection_manager,
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
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    jsonrpc_client: JsonRpcClient,
    start_block: Option<u64>,
) -> Result<()> {
    info!("Starting NEAR indexer");

    let lake_config = create_lake_config(
        config,
        redis_connection_manager,
        &jsonrpc_client,
        start_block,
    )
    .await?;
    let (_, stream) = near_lake_framework::streamer(lake_config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    stream
        .map(move |streamer_message| {
            let config = config.clone();
            let mut redis_connection_manager = redis_connection_manager.clone();

            async move {
                handle_streamer_message(
                    &config,
                    &mut redis_connection_manager,
                    &streamer_message,
                )
                .await;

                utils::redis::update_last_processed(
                    &config,
                    &mut redis_connection_manager,
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

async fn handle_streamer_message(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    streamer_message: &StreamerMessage,
) {
    let nep_locker_event_outcomes = find_nep_locker_event_outcomes(config, streamer_message);

    for outcome in nep_locker_event_outcomes {
        let receipt_id = outcome.receipt.receipt_id.to_string();

        for log in outcome.execution_outcome.outcome.logs {
            let Ok(log) = serde_json::from_str::<OmniBridgeEvent>(&log) else {
                continue;
            };

            info!("Received OmniBridgeEvent: {}", log.to_log_string());

            match log {
                OmniBridgeEvent::InitTransferEvent { transfer_message }
                | OmniBridgeEvent::UpdateFeeEvent { transfer_message } => {
                    let origin_nonce = transfer_message.origin_nonce.to_string();
                    let key = utils::redis::composite_key(&[&receipt_id, &origin_nonce]);

                    utils::redis::add_event(
                        config,
                        redis_connection_manager,
                        utils::redis::EVENTS,
                        key,
                        RetryableEvent::new(crate::workers::Transfer::Near { transfer_message }),
                    )
                    .await;
                }
                OmniBridgeEvent::UtxoTransferEvent {
                    utxo_transfer_message,
                    new_transfer_id,
                    ..
                } => {
                    if let Some(new_transfer_id) = new_transfer_id {
                        let utxo_id = utxo_transfer_message.utxo_id.to_string();
                        let key = utils::redis::composite_key(&[&receipt_id, &utxo_id]);

                        utils::redis::add_event(
                            config,
                            redis_connection_manager,
                            utils::redis::EVENTS,
                            key,
                            RetryableEvent::new(crate::workers::Transfer::Utxo {
                                utxo_transfer_message,
                                new_transfer_id: new_transfer_id.into(),
                            }),
                        )
                        .await;
                    }
                }
                OmniBridgeEvent::SignTransferEvent {
                    ref message_payload,
                    ..
                } => {
                    let origin_nonce = message_payload.transfer_id.origin_nonce.to_string();
                    let key = utils::redis::composite_key(&[&receipt_id, &origin_nonce]);

                    utils::redis::add_event(
                        config,
                        redis_connection_manager,
                        utils::redis::EVENTS,
                        key,
                        RetryableEvent::new(log),
                    )
                    .await;
                }
                OmniBridgeEvent::FinTransferEvent { transfer_message } => {
                    if transfer_message.recipient.get_chain() != ChainKind::Near {
                        let origin_nonce = transfer_message.origin_nonce.to_string();
                        let key = utils::redis::composite_key(&[&receipt_id, &origin_nonce]);

                        utils::redis::add_event(
                            config,
                            redis_connection_manager,
                            utils::redis::EVENTS,
                            key,
                            RetryableEvent::new(crate::workers::Transfer::Near {
                                transfer_message,
                            }),
                        )
                        .await;
                    }
                }
                OmniBridgeEvent::FailedFinTransferEvent { .. }
                | OmniBridgeEvent::FastTransferEvent { .. }
                | OmniBridgeEvent::ClaimFeeEvent { .. }
                | OmniBridgeEvent::LogMetadataEvent { .. }
                | OmniBridgeEvent::DeployTokenEvent { .. }
                | OmniBridgeEvent::MigrateTokenEvent { .. }
                | OmniBridgeEvent::BindTokenEvent { .. } => {}
            }
        }
    }
}

fn find_nep_locker_event_outcomes(
    config: &config::Config,
    streamer_message: &StreamerMessage,
) -> Vec<IndexerExecutionOutcomeWithReceipt> {
    streamer_message
        .shards
        .iter()
        .flat_map(|shard| shard.receipt_execution_outcomes.iter())
        .filter(|outcome| is_nep_locker_event(config, &outcome.receipt))
        .cloned()
        .collect()
}

fn is_nep_locker_event(config: &config::Config, receipt: &ReceiptView) -> bool {
    receipt.receiver_id == config.near.omni_bridge_id
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { .. })
            })
        )
}

pub async fn get_final_block(jsonrpc_client: &JsonRpcClient) -> Result<u64> {
    info!("Getting final block");

    let block_response = RpcBlockRequest {
        block_reference: near_primitives::types::BlockReference::Finality(
            near_primitives::types::Finality::Final,
        ),
    };

    jsonrpc_client
        .call(block_response)
        .await
        .map(|block| block.header.height)
        .map_err(Into::into)
}
