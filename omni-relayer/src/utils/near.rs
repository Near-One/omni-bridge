use anyhow::Result;
use tracing::{info, warn};

use near_jsonrpc_client::{JsonRpcClient, methods::block::RpcBlockRequest};
use near_lake_framework::near_indexer_primitives::{
    IndexerExecutionOutcomeWithReceipt, StreamerMessage,
    views::{ActionView, ReceiptEnumView, ReceiptView},
};
use near_primitives::{hash::CryptoHash, types::AccountId};
use omni_types::{ChainKind, near_events::OmniBridgeEvent};

use crate::{config, utils, workers::RetryableEvent};

pub const RETRY_ATTEMPTS: u64 = 10;
pub const RETRY_SLEEP_SECS: u64 = 5;

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

pub async fn is_tx_successful(
    jsonrpc_client: &JsonRpcClient,
    tx_hash: CryptoHash,
    sender_account_id: AccountId,
    specific_errors: Option<Vec<String>>,
) -> bool {
    let request = near_jsonrpc_client::methods::tx::RpcTransactionStatusRequest {
        transaction_info: near_jsonrpc_client::methods::tx::TransactionInfo::TransactionId {
            tx_hash,
            sender_account_id,
        },
        wait_until: near_primitives::views::TxExecutionStatus::Final,
    };

    let mut response = None;

    for _ in 0..RETRY_ATTEMPTS {
        if let Ok(res) = jsonrpc_client.call(&request).await {
            response = Some(res);
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_SLEEP_SECS)).await;
    }

    let Some(response) = response else {
        warn!("Failed to get transaction status");
        return false;
    };

    if let Some(near_primitives::views::FinalExecutionOutcomeViewEnum::FinalExecutionOutcome(
        final_execution_outcome,
    )) = response.final_execution_outcome
    {
        for receipt_outcome in final_execution_outcome.receipts_outcome {
            if let near_primitives::views::ExecutionStatusView::Failure(tx_execution_error) =
                receipt_outcome.outcome.status
            {
                warn!(
                    "Found failed receipt in the transaction ({tx_hash}): {tx_execution_error:?}"
                );

                if let Some(ref specific_errors) = specific_errors {
                    if specific_errors.iter().any(|specific_error| {
                        tx_execution_error.to_string().contains(specific_error)
                    }) {
                        info!(
                            "Transaction ({tx_hash}) failed with specific error: {tx_execution_error:?}"
                        );
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
    }

    true
}

pub async fn handle_streamer_message(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    streamer_message: &StreamerMessage,
) {
    let nep_locker_event_outcomes = find_nep_locker_event_outcomes(config, streamer_message);

    let nep_locker_event_logs = nep_locker_event_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<OmniBridgeEvent>(&log).ok())
        .collect::<Vec<_>>();

    for log in nep_locker_event_logs {
        info!("Received OmniBridgeEvent: {}", log.to_log_string());

        match log {
            OmniBridgeEvent::InitTransferEvent { transfer_message }
            | OmniBridgeEvent::UpdateFeeEvent { transfer_message } => {
                utils::redis::add_event(
                    config,
                    redis_connection_manager,
                    utils::redis::EVENTS,
                    transfer_message.origin_nonce.to_string(),
                    RetryableEvent::new(crate::workers::Transfer::Near { transfer_message }),
                )
                .await;
            }
            OmniBridgeEvent::SignTransferEvent {
                ref message_payload,
                ..
            } => {
                utils::redis::add_event(
                    config,
                    redis_connection_manager,
                    utils::redis::EVENTS,
                    message_payload.transfer_id.origin_nonce.to_string(),
                    RetryableEvent::new(log),
                )
                .await;
            }
            OmniBridgeEvent::FinTransferEvent { transfer_message } => {
                if transfer_message.recipient.get_chain() != ChainKind::Near {
                    utils::redis::add_event(
                        config,
                        redis_connection_manager,
                        utils::redis::EVENTS,
                        transfer_message.origin_nonce.to_string(),
                        RetryableEvent::new(crate::workers::Transfer::Near { transfer_message }),
                    )
                    .await;
                }
            }
            OmniBridgeEvent::FailedFinTransferEvent { .. }
            | OmniBridgeEvent::FastTransferEvent { .. }
            | OmniBridgeEvent::ClaimFeeEvent { .. }
            | OmniBridgeEvent::LogMetadataEvent { .. }
            | OmniBridgeEvent::DeployTokenEvent { .. }
            | OmniBridgeEvent::BindTokenEvent { .. } => {}
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
