use anyhow::Result;
use log::{info, warn};

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

#[derive(BorshDeserialize)]
struct EthLightClientResponse {
    last_block_number: u64,
}

pub async fn get_eth_light_client_last_block_number(
    config: &config::Config,
    jsonrpc_client: &JsonRpcClient,
) -> Result<u64> {
    let request = methods::query::RpcQueryRequest {
        block_reference: BlockReference::latest(),
        request: QueryRequest::CallFunction {
            account_id: config.near.eth_light_client.clone(),
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

                            utils::redis::add_event(
                                redis_connection,
                                utils::redis::NEAR_BAD_FEE_EVENTS,
                                transfer_message.origin_nonce.0.to_string(),
                                log,
                            )
                            .await;
                        }
                    }
                    Err(err) => {
                        warn!("Failed to check fee: {}", err);

                        utils::redis::add_event(
                            redis_connection,
                            utils::redis::NEAR_BAD_FEE_EVENTS,
                            transfer_message.origin_nonce.0.to_string(),
                            log,
                        )
                        .await;
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
            } => {
                if nonce.is_none() {
                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::NEAR_FIN_TRANSFER_EVENTS,
                        transfer_message.origin_nonce.0.to_string(),
                        log,
                    )
                    .await;
                }
            }
            Nep141LockerEvent::ClaimFeeEvent {
                ref transfer_message,
                ref native_fee_recipient,
            } => {
                if native_fee_recipient == &config.evm.relayer {
                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::NEAR_FIN_TRANSFER_EVENTS,
                        transfer_message.origin_nonce.0.to_string(),
                        log,
                    )
                    .await;
                }
            }
            Nep141LockerEvent::SignClaimNativeFeeEvent {
                ref claim_payload, ..
            } => {
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
            Nep141LockerEvent::LogMetadataEvent { .. } => {
                info!("Received LogMetadataEvent");
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
    receipt.receiver_id == config.near.token_locker_id
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { .. })
            })
        )
}
