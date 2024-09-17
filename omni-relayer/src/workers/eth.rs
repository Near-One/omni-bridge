use std::sync::Arc;

use alloy::{rpc::types::Log, sol};

use futures::future::join_all;
use nep141_connector::Nep141Connector;

use crate::{config, utils};

sol!(
    #[derive(Debug)]
    event Withdraw(
        string token,
        address indexed sender,
        uint256 amount,
        string recipient,
        address indexed tokenEthAddress
    );
);

pub async fn finalize_withdraw(
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
        while let Some((key, event)) = events.next_item().await {
            if let Ok(event) = serde_json::from_str::<Log>(&event) {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        if let Ok(decoded_log) = event.log_decode::<Withdraw>() {
                            log::info!("Decoded log: {:?}", decoded_log);

                            let Some(tx_hash) = decoded_log.transaction_hash else {
                                log::warn!("No transaction hash in log: {:?}", event);
                                return;
                            };
                            let Some(log_index) = decoded_log.log_index else {
                                log::warn!("No log index in log: {:?}", event);
                                return;
                            };

                            match connector
                                .finalize_withdraw(
                                    primitive_types::H256::from_slice(tx_hash.as_slice()),
                                    log_index,
                                )
                                .await
                            {
                                Ok(tx_hash) => {
                                    log::info!("Finalized withdraw: {:?}", tx_hash);
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        &config.redis.near_sign_transfer_events,
                                        key,
                                    )
                                    .await;
                                }
                                Err(err) => log::error!("Failed to finalize withdraw: {}", err),
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
