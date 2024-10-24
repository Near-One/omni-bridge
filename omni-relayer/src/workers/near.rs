use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use log::{error, info, warn};

use near_primitives::borsh;
use omni_connector::OmniConnector;
use omni_types::{locker_args::ClaimFeeArgs, near_events::Nep141LockerEvent};

use crate::{config, utils};

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
            if let Ok(event) = serde_json::from_str::<Nep141LockerEvent>(&event) {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let (Nep141LockerEvent::InitTransferEvent { transfer_message }
                        | Nep141LockerEvent::FinTransferEvent {
                            transfer_message, ..
                        }) = event
                        else {
                            warn!(
                                "Expected InitTransferEvent/FinTransferEvent, got: {:?}",
                                event
                            );
                            return;
                        };

                        info!(
                            "Received InitTransferEvent/FinTransferEvent: {}",
                            transfer_message.origin_nonce.0
                        );

                        match connector
                            .sign_transfer(
                                transfer_message.origin_nonce.into(),
                                Some(config.near.token_locker_id),
                                Some(transfer_message.fee),
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
                        let Nep141LockerEvent::SignTransferEvent { .. } = &event else {
                            error!("Expected SignTransferEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received SignTransferEvent");

                        match connector.evm_fin_transfer_with_log(event).await {
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
            if let Ok((block_number, deposit_log)) = serde_json::from_str::<(u64, Vec<u8>)>(&event)
            {
                let Ok(light_client_latest_block_number) =
                    utils::near::get_eth_light_client_last_block_number(&config, &jsonrpc_client)
                        .await
                else {
                    warn!("Failed to get eth light client last block number");
                    continue;
                };

                if block_number > light_client_latest_block_number {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                    ))
                    .await;
                    continue;
                }

                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        info!("Received finalized transfer");

                        let Ok(claim_fee_args) = borsh::from_slice::<ClaimFeeArgs>(&deposit_log)
                        else {
                            warn!("Failed to decode claim fee args");
                            return;
                        };

                        if let Ok(response) = connector.claim_fee(claim_fee_args).await {
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

pub async fn sign_claim_native_fee(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_QUEUE.to_string(),
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
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let Nep141LockerEvent::ClaimFeeEvent {
                            ref transfer_message,
                            ..
                        } = event
                        else {
                            warn!("Expected ClaimFeeEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received ClaimFeeEvent log");

                        match connector
                            .sign_claim_native_fee(
                                vec![transfer_message.origin_nonce.into()],
                                config.evm.relayer_address_on_eth,
                            )
                            .await
                        {
                            Ok(tx_hash) => {
                                info!("Signed claiming native fee: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_QUEUE,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => error!("Failed to sign claiming native fee: {}", err),
                        };
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
