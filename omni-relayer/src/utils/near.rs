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
            }
            | Nep141LockerEvent::UpdateFeeEvent {
                ref transfer_message,
            } => {
                match utils::fee::is_fee_sufficient(
                    jsonrpc_client,
                    &transfer_message.sender,
                    &transfer_message.recipient,
                    &transfer_message.token,
                    transfer_message.fee.fee.into(),
                )
                .await
                {
                    Ok(res) => {
                        if res {
                            utils::redis::add_event(
                                redis_connection,
                                utils::redis::NEAR_INIT_TRANSFER_EVENTS,
                                transfer_message.origin_nonce.0.to_string(),
                                log,
                            )
                            .await;
                        } else {
                            warn!("Fee is not sufficient for transfer: {:?}", transfer_message);
                        }
                    }
                    Err(err) => {
                        warn!("Failed to check fee: {}", err);
                    }
                }
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
            Nep141LockerEvent::FinTransferEvent {
                ref nonce,
                ref transfer_message,
            } => match nonce {
                Some(_) => {
                    match utils::fee::is_fee_sufficient(
                        jsonrpc_client,
                        &transfer_message.sender,
                        &transfer_message.recipient,
                        &transfer_message.token,
                        transfer_message.fee.fee.into(),
                    )
                    .await
                    {
                        Ok(res) => {
                            if res {
                                utils::redis::add_event(
                                    redis_connection,
                                    utils::redis::NEAR_INIT_TRANSFER_EVENTS,
                                    transfer_message.origin_nonce.0.to_string(),
                                    log,
                                )
                                .await;
                            } else {
                                warn!("Fee is not sufficient for transfer: {:?}", transfer_message);
                            }
                        }
                        Err(err) => {
                            warn!("Failed to check fee: {}", err);
                        }
                    }
                }
                None => {
                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_QUEUE,
                        transfer_message.origin_nonce.0.to_string(),
                        log,
                    )
                    .await;
                }
            },
            Nep141LockerEvent::ClaimFeeEvent {
                ref transfer_message,
                ref native_fee_recipient,
            } => {
                if native_fee_recipient == &config.near.relayer_address_on_evm {
                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_QUEUE,
                        transfer_message.origin_nonce.0.to_string(),
                        log,
                    )
                    .await;
                }
            }
            Nep141LockerEvent::SignClaimNativeFeeEvent {
                ref claim_payload, ..
            } => {
                if claim_payload.recipient == config.near.relayer_address_on_evm {
                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_EVENTS,
                        claim_payload
                            .nonces
                            .iter()
                            .map(|nonce| nonce.0.to_string())
                            .collect::<Vec<_>>()
                            .join(","),
                        log,
                    )
                    .await;
                }
            }
            Nep141LockerEvent::LogMetadataEvent { .. } => {}
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
