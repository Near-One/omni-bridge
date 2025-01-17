use anyhow::{Context, Result};
use log::info;

use near_jsonrpc_client::{
    methods::{self, block::RpcBlockRequest},
    JsonRpcClient,
};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_lake_framework::near_indexer_primitives::{
    views::{ActionView, ReceiptEnumView, ReceiptView},
    IndexerExecutionOutcomeWithReceipt, StreamerMessage,
};
use near_primitives::{
    borsh::{from_slice, BorshDeserialize},
    types::BlockReference,
    views::QueryRequest,
};
use omni_types::{near_events::OmniBridgeEvent, ChainKind};

use crate::{config, utils};

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

#[derive(BorshDeserialize)]
struct EthLightClientResponse {
    last_block_number: u64,
}

pub async fn get_eth_light_client_last_block_number(
    config: &config::Config,
    jsonrpc_client: &JsonRpcClient,
) -> Result<u64> {
    let Some(ref eth) = config.eth else {
        anyhow::bail!("Failed to get ETH light client");
    };

    let request = methods::query::RpcQueryRequest {
        block_reference: BlockReference::latest(),
        request: QueryRequest::CallFunction {
            account_id: eth
                .light_client
                .clone()
                .context("Failed to get ETH light client")?,
            method_name: "last_block_number".to_string(),
            args: Vec::new().into(),
        },
    };

    let response = jsonrpc_client.call(request).await?;

    if let QueryResponseKind::CallResult(result) = response.kind {
        Ok(from_slice::<EthLightClientResponse>(&result.result)?.last_block_number)
    } else {
        anyhow::bail!("Failed to get token decimals")
    }
}

pub async fn handle_streamer_message(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    streamer_message: &StreamerMessage,
) {
    let nep_locker_event_outcomes = find_nep_locker_event_outcomes(config, streamer_message);

    let nep_locker_event_logs = nep_locker_event_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<OmniBridgeEvent>(&log).ok())
        .collect::<Vec<_>>();

    for log in nep_locker_event_logs {
        info!("Processing OmniBridgeEvent: {:?}", log);

        match log {
            OmniBridgeEvent::InitTransferEvent {
                ref transfer_message,
            }
            | OmniBridgeEvent::UpdateFeeEvent {
                ref transfer_message,
            } => {
                utils::redis::add_event(
                    redis_connection,
                    utils::redis::NEAR_INIT_TRANSFER_QUEUE,
                    transfer_message.origin_nonce.to_string(),
                    crate::workers::near::InitTransferWithTimestamp {
                        event: log,
                        creation_timestamp: chrono::Utc::now().timestamp(),
                        last_update_timestamp: None,
                    },
                )
                .await;
            }
            OmniBridgeEvent::SignTransferEvent {
                ref message_payload,
                ..
            } => {
                utils::redis::add_event(
                    redis_connection,
                    utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
                    message_payload.destination_nonce.to_string(),
                    log,
                )
                .await;
            }
            OmniBridgeEvent::FinTransferEvent {
                ref transfer_message,
            } => {
                if transfer_message.recipient.get_chain() != ChainKind::Near {
                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::NEAR_INIT_TRANSFER_QUEUE,
                        transfer_message.origin_nonce.to_string(),
                        crate::workers::near::InitTransferWithTimestamp {
                            event: log,
                            creation_timestamp: chrono::Utc::now().timestamp(),
                            last_update_timestamp: None,
                        },
                    )
                    .await;
                }
            }
            OmniBridgeEvent::ClaimFeeEvent { .. } | OmniBridgeEvent::LogMetadataEvent { .. } => {}
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
    receipt.receiver_id == config.near.token_locker_id
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { .. })
            })
        )
}
