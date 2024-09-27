use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use log::warn;

use near_primitives::borsh;
use omni_connector::OmniConnector;
use omni_types::locker_args::FinTransferArgs;

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
                        log::info!("Received FinTransfer log");

                        let Ok(fin_transfer_args) =
                            borsh::from_slice::<FinTransferArgs>(&withdraw_log)
                        else {
                            warn!("Failed to decode log: {:?}", withdraw_log);
                            return;
                        };

                        match connector.near_fin_transfer(fin_transfer_args).await {
                            Ok(tx_hash) => {
                                log::info!("Finalized withdraw: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::ETH_WITHDRAW_EVENTS,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => log::error!("Failed to finalize withdraw: {}", err),
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
