use anyhow::{Context, Result};
use log::{info, warn};
use tokio::sync::mpsc;

use near_jsonrpc_client::{methods::block::RpcBlockRequest, JsonRpcClient};
use near_lake_framework::near_indexer_primitives::{
    views::{ActionView, ReceiptEnumView, ReceiptView},
    IndexerExecutionOutcomeWithReceipt, StreamerMessage,
};
use near_primitives::types::AccountId;
use omni_types::near_events::Nep141LockerEvent;

use crate::config;

pub async fn get_final_block(client: &JsonRpcClient) -> Result<u64> {
    info!("Getting final block");

    let block_response = RpcBlockRequest {
        block_reference: near_primitives::types::BlockReference::Finality(
            near_primitives::types::Finality::Final,
        ),
    };
    client
        .call(block_response)
        .await
        .map(|block| block.header.height)
        .map_err(Into::into)
}

pub fn handle_streamer_message(
    streamer_message: StreamerMessage,
    sign_tx: &mpsc::UnboundedSender<Nep141LockerEvent>,
    finalize_transfer_tx: &mpsc::UnboundedSender<Nep141LockerEvent>,
) {
    let nep_locker_event_outcomes = find_nep_locker_event_outcomes(streamer_message);

    let nep_locker_event_logs = nep_locker_event_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<Nep141LockerEvent>(&log).ok())
        .collect::<Vec<_>>();

    for log in nep_locker_event_logs {
        info!("Processing Nep141LockerEvent: {:?}", log);

        match log {
            Nep141LockerEvent::InitTransferEvent { .. } => {
                if let Err(err) = sign_tx.send(log) {
                    warn!("Failed to send InitTransferEvent to sign_tx: {}", err);
                }
            }
            Nep141LockerEvent::SignTransferEvent { .. } => {
                if let Err(err) = finalize_transfer_tx.send(log) {
                    warn!(
                        "Failed to send SignTransferEvent to finalize_transfer_tx: {}",
                        err
                    );
                }
            }
        }
    }
}

fn find_nep_locker_event_outcomes(
    streamer_message: StreamerMessage,
) -> Vec<IndexerExecutionOutcomeWithReceipt> {
    streamer_message
        .shards
        .iter()
        .flat_map(|shard| shard.receipt_execution_outcomes.iter())
        .filter(|outcome| is_nep_locker_event(&outcome.receipt).map_or(false, |res| res))
        .cloned()
        .collect()
}

fn is_nep_locker_event(receipt: &ReceiptView) -> Result<bool> {
    Ok(receipt.receiver_id
        == config::TOKEN_LOCKER_ID_TESTNET
            .parse::<AccountId>()
            .context("Failed to parse AccountId")?
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "ft_on_transfer" || method_name == "sign_transfer_callback")
            })
        ))
}
