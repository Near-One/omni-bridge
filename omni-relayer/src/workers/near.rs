use std::sync::Arc;

use alloy::rpc::types::{Log, TransactionReceipt};
use anyhow::Result;
use ethereum_types::H256;
use futures::future::join_all;
use log::{error, info, warn};

use omni_connector::OmniConnector;
use omni_types::{
    locker_args::ClaimFeeArgs, near_events::Nep141LockerEvent, prover_result::ProofKind, ChainKind,
};

use crate::{config, utils};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitTransferWithTimestamp {
    pub event: Nep141LockerEvent,
    pub creation_timestamp: i64,
    pub last_update_timestamp: Option<i64>,
}

pub async fn sign_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_INIT_TRANSFER_QUEUE.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        for (key, event) in events {
            if let Ok(init_transfer_with_timestamp) =
                serde_json::from_str::<InitTransferWithTimestamp>(&event)
            {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let current_timestamp = chrono::Utc::now().timestamp();

                        if current_timestamp
                            - init_transfer_with_timestamp
                                .last_update_timestamp
                                .unwrap_or_default()
                            < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
                        {
                            return;
                        }

                        let (Nep141LockerEvent::InitTransferEvent {
                            ref transfer_message,
                        }
                        | Nep141LockerEvent::FinTransferEvent {
                            ref transfer_message,
                            ..
                        }
                        | Nep141LockerEvent::UpdateFeeEvent {
                            ref transfer_message,
                        }) = init_transfer_with_timestamp.event
                        else {
                            warn!(
                                "Expected InitTransferEvent/FinTransferEvent/UpdateFeeEvent, got: {:?}",
                                event
                            );
                            return;
                        };

                        info!(
                            "Received InitTransferEvent/FinTransferEvent/UpdateFeeEvent",
                        );

                        if current_timestamp - init_transfer_with_timestamp.creation_timestamp
                            > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
                        {
                            warn!(
                                "Removing an old InitTransfer: {:?}",
                                init_transfer_with_timestamp
                            );
                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::NEAR_INIT_TRANSFER_QUEUE,
                                &key,
                            )
                            .await;
                            return;
                        }

                        // TODO: Use existing API to check if fee is sufficient

                        match connector
                            .near_sign_transfer(
                                transfer_message.origin_nonce,
                                Some(config.near.token_locker_id),
                                Some(transfer_message.fee.clone()),
                            )
                            .await
                        {
                            Ok(outcome) => {
                                info!("Signed transfer: {:?}", outcome.transaction.hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_INIT_TRANSFER_QUEUE,
                                    &key,
                                )
                                .await;
                            }
                            Err(err) => {
                                error!("Failed to sign transfer: {}", err);
                            }
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

pub async fn finalize_transfer(
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_SIGN_TRANSFER_EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        for (key, event) in events {
            if let Ok(event) = serde_json::from_str::<Nep141LockerEvent>(&event) {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let Nep141LockerEvent::SignTransferEvent {
                            message_payload, ..
                        } = &event
                        else {
                            error!("Expected SignTransferEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received SignTransferEvent");

                        let fin_transfer_args = match message_payload.recipient.get_chain() {
                            ChainKind::Eth => omni_connector::FinTransferArgs::EvmFinTransfer {
                                chain_kind: message_payload.recipient.get_chain(),
                                event,
                            },
                            _ => todo!(),
                        };

                        match connector.fin_transfer(fin_transfer_args).await {
                            Ok(tx_hash) => {
                                info!("Finalized deposit: {}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
                                    &key,
                                )
                                .await;
                            }
                            Err(err) => {
                                error!("Failed to finalize deposit: {}", err);
                            }
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FinTransfer {
    pub chain_kind: ChainKind,
    pub block_number: u64,
    pub log: Log,
    pub tx_logs: Option<TransactionReceipt>,
}

pub async fn claim_fee(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::FINALIZED_TRANSFERS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(fin_transfer) = serde_json::from_str::<FinTransfer>(&event) {
                let vaa = utils::evm::get_vaa(
                    connector.clone(),
                    fin_transfer.chain_kind,
                    fin_transfer.tx_logs,
                    &config,
                )
                .await;

                if vaa.is_none() {
                    let Ok(light_client_latest_block_number) =
                        utils::near::get_eth_light_client_last_block_number(
                            &config,
                            &jsonrpc_client,
                        )
                        .await
                    else {
                        warn!("Failed to get eth light client last block number");
                        continue;
                    };

                    if fin_transfer.block_number > light_client_latest_block_number {
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                        ))
                        .await;
                        continue;
                    }
                }

                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        info!("Received finalized transfer");

                        let Some(tx_hash) = fin_transfer.log.transaction_hash else {
                            warn!("No transaction hash in log: {:?}", fin_transfer.log);
                            return;
                        };

                        let Some(topic) = fin_transfer.log.topic0() else {
                            warn!("No topic0 in log: {:?}", fin_transfer.log);
                            return;
                        };

                        let tx_hash = H256::from_slice(tx_hash.as_slice());

                        let Some(prover_args) = utils::evm::construct_prover_args(
                            &config,
                            vaa,
                            tx_hash,
                            H256::from_slice(topic.as_slice()),
                            ProofKind::FinTransfer,
                        )
                        .await
                        else {
                            warn!("Failed to get prover args");
                            return;
                        };

                        let claim_fee_args = ClaimFeeArgs {
                            chain_kind: fin_transfer.chain_kind,
                            prover_args,
                        };

                        if let Ok(response) = connector.near_claim_fee(claim_fee_args).await {
                            info!("Claimed fee: {:?}", response);
                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::FINALIZED_TRANSFERS,
                                &key,
                            )
                            .await;
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}
