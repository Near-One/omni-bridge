use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;

use alloy::rpc::types::Log;
use ethereum_types::H256;
use nep141_connector::Nep141Connector;

use crate::utils;

pub async fn finalize_withdraw(
    redis_client: redis::Client,
    connector: Arc<Nep141Connector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events(
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
        while let Some((key, event)) = events.next_item().await {
            if let Ok(withdraw_log) =
                serde_json::from_str::<Log<crate::startup::eth::Withdraw>>(&event)
            {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        log::info!("Decoded log: {:?}", withdraw_log);

                        let Some(tx_hash) = withdraw_log.transaction_hash else {
                            log::warn!("No transaction hash in log: {:?}", withdraw_log);
                            return;
                        };
                        let Some(log_index) = withdraw_log.log_index else {
                            log::warn!("No log index in log: {:?}", withdraw_log);
                            return;
                        };

                        match connector
                            .finalize_withdraw(H256::from_slice(tx_hash.as_slice()), log_index)
                            .await
                        {
                            Ok(tx_hash) => {
                                log::info!("Finalized withdraw: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
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
