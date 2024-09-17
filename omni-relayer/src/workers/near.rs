use std::sync::Arc;

use futures::future::join_all;
use log::{error, info, warn};
use nep141_connector::Nep141Connector;

use omni_types::near_events::Nep141LockerEvent;

use crate::{config, utils};

pub async fn sign_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<Nep141Connector>,
) {
    let redis_connection = redis_client
        .get_multiplexed_tokio_connection()
        .await
        .unwrap();

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events(
            &mut redis_connection_clone,
            config.redis.near_init_transfer_events.clone(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                config.redis.sleep_time_after_events_process_secs,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        while let Some((nonce, event)) = events.next_item().await {
            if let Ok(event) = serde_json::from_str::<Nep141LockerEvent>(&event) {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let Nep141LockerEvent::InitTransferEvent { transfer_message } = event
                        else {
                            warn!("Expected InitTransferEvent, got: {:?}", event);
                            return;
                        };

                        info!(
                            "Received InitTransferEvent: {}",
                            transfer_message.origin_nonce.0
                        );

                        match connector
                            .sign_transfer(
                                transfer_message.origin_nonce.into(),
                                Some(config.testnet.token_locker_id),
                                transfer_message.fee.into(),
                            )
                            .await
                        {
                            Ok(outcome) => {
                                info!("Signed transfer: {:?}", outcome);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    &config.redis.near_init_transfer_events,
                                    &nonce,
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
            config.redis.sleep_time_after_events_process_secs,
        ))
        .await;
    }
}

pub async fn finalize_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<Nep141Connector>,
) {
    let redis_connection = redis_client
        .get_multiplexed_tokio_connection()
        .await
        .unwrap();

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events(
            &mut redis_connection_clone,
            config.redis.near_sign_transfer_events.clone(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                config.redis.sleep_time_after_events_process_secs,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        while let Some((nonce, event)) = events.next_item().await {
            if let Ok(event) = serde_json::from_str::<Nep141LockerEvent>(&event) {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let Nep141LockerEvent::SignTransferEvent { .. } = &event else {
                            error!("Expected SignTransferEvent, got: {:?}", event);
                            return;
                        };

                        match connector.finalize_deposit_omni_with_log(event).await {
                            Ok(tx_hash) => {
                                info!("Finalized deposit: {}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    &config.redis.near_sign_transfer_events,
                                    &nonce,
                                )
                                .await;
                                println!(
                                    "Adding event: {} {} {}",
                                    nonce, config.redis.eth_finalized_transfer_events, tx_hash
                                );
                                utils::redis::add_event(
                                    &mut redis_connection,
                                    &nonce,
                                    &config.redis.eth_finalized_transfer_events,
                                    tx_hash,
                                )
                                .await;
                                println!("Added event");
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
            config.redis.sleep_time_after_events_process_secs,
        ))
        .await;
    }
}

pub async fn claim_fee(config: config::Config, redis_client: redis::Client) {
    let redis_connection = redis_client
        .get_multiplexed_tokio_connection()
        .await
        .unwrap();

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events(
            &mut redis_connection_clone,
            config.redis.eth_finalized_transfer_events.clone(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                config.redis.sleep_time_after_events_process_secs,
            ))
            .await;
            continue;
        };

        while let Some((nonce, event)) = events.next_item().await {
            if let Ok(event) = serde_json::from_str::<primitive_types::H256>(&event) {
                info!("Event: {:?}", event);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.redis.sleep_time_after_events_process_secs,
        ))
        .await;
    }
}
