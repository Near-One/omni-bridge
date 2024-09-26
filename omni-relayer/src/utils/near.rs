use anyhow::Result;
use log::{info, warn};

use near_jsonrpc_client::{methods::block::RpcBlockRequest, JsonRpcClient};
use near_lake_framework::near_indexer_primitives::{
    views::{ActionView, ReceiptEnumView, ReceiptView},
    IndexerExecutionOutcomeWithReceipt, StreamerMessage,
};
use omni_types::near_events::Nep141LockerEvent;

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

pub async fn handle_streamer_message(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: &JsonRpcClient,
    streamer_message: &StreamerMessage,
) {
    let nep_locker_event_outcomes = find_nep_locker_event_outcomes(config, streamer_message);

    let nep_locker_event_logs = nep_locker_event_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<Nep141LockerEvent>(&log).ok())
        .collect::<Vec<_>>();

    for log in nep_locker_event_logs {
        info!("Processing Nep141LockerEvent: {:?}", log);

        match log {
            Nep141LockerEvent::InitTransferEvent {
                ref transfer_message,
            } => {
                // TODO: If fee is insufficient, it should be handled later. For example,
                // add to redis and try again in 1 hour
                match utils::price::is_fee_sufficient(
                    jsonrpc_client,
                    &transfer_message.sender,
                    &transfer_message.recipient,
                    &transfer_message.token,
                    transfer_message.fee.into(),
                )
                .await
                {
                    Ok(res) => {
                        if !res {
                            warn!("Fee is insufficient");
                        }
                    }
                    Err(err) => {
                        warn!("Failed to check fee: {}", err);
                    }
                }

                utils::redis::add_event(
                    redis_connection,
                    utils::redis::NEAR_INIT_TRANSFER_EVENTS,
                    transfer_message.origin_nonce.0.to_string(),
                    log,
                )
                .await;
            }
            Nep141LockerEvent::SignTransferEvent {
                ref message_payload,
                ..
            } => {
                utils::redis::add_event(
                    redis_connection,
                    utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
                    message_payload.nonce.0.to_string(),
                    log,
                )
                .await;
            }
            Nep141LockerEvent::FinTransferEvent { .. }
            | Nep141LockerEvent::UpdateFeeEvent { .. }
            | Nep141LockerEvent::LogMetadataEvent { .. } => todo!(),
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
                matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "ft_on_transfer" || method_name == "sign_transfer_callback")
            })
        )
}
