use anyhow::Result;
use log::{info, warn};
use tokio::sync::mpsc;

use near_jsonrpc_client::{methods::block::RpcBlockRequest, JsonRpcClient};
use near_lake_framework::near_indexer_primitives::{
    views::{ActionView, ReceiptEnumView, ReceiptView},
    IndexerExecutionOutcomeWithReceipt, StreamerMessage,
};
use omni_types::near_events::Nep141LockerEvent;

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
    config: &crate::Config,
    streamer_message: StreamerMessage,
    sign_tx: &mpsc::UnboundedSender<Nep141LockerEvent>,
    finalize_transfer_tx: &mpsc::UnboundedSender<Nep141LockerEvent>,
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
            Nep141LockerEvent::FinTransferEvent { .. }
            | Nep141LockerEvent::UpdateFeeEvent { .. } => todo!(),
        }
    }
}

fn find_nep_locker_event_outcomes(
    config: &crate::Config,
    streamer_message: StreamerMessage,
) -> Vec<IndexerExecutionOutcomeWithReceipt> {
    streamer_message
        .shards
        .iter()
        .flat_map(|shard| shard.receipt_execution_outcomes.iter())
        .filter(|outcome| is_nep_locker_event(config, &outcome.receipt))
        .cloned()
        .collect()
}

fn is_nep_locker_event(config: &crate::Config, receipt: &ReceiptView) -> bool {
    receipt.receiver_id == config.token_locker_id_testnet
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "ft_on_transfer" || method_name == "sign_transfer_callback")
            })
        )
}
