use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use log::{error, info, warn};

use near_primitives::borsh;
use omni_connector::OmniConnector;
use omni_types::{locker_args::FinTransferArgs, near_events::Nep141LockerEvent};

use crate::utils;

pub async fn finalize_withdraw(
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::ETH_WITHDRAW_EVENTS.to_string(),
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
            if let Ok(withdraw_log) = serde_json::from_str::<Vec<u8>>(&event) {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let Ok(fin_transfer_args) =
                            borsh::from_slice::<FinTransferArgs>(&withdraw_log)
                        else {
                            warn!("Failed to decode log: {:?}", withdraw_log);
                            return;
                        };

                        info!("Received FinTransfer log");

                        match connector.near_fin_transfer(fin_transfer_args).await {
                            Ok(tx_hash) => {
                                info!("Finalized withdraw: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::ETH_WITHDRAW_EVENTS,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => error!("Failed to finalize withdraw: {}", err),
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

pub async fn claim_native_fee(
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_EVENTS.to_string(),
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
                        let Nep141LockerEvent::SignClaimNativeFeeEvent { .. } = event else {
                            warn!("Expected SignClaimNativeFeeEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received SignClaimNativeFeeEvent log");

                        match connector.evm_claim_native_fee_with_log(event).await {
                            Ok(tx_hash) => {
                                info!("Claimed native fee: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_EVENTS,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => error!("Failed to claim native fee: {}", err),
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
