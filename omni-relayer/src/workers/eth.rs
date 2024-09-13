use std::sync::Arc;

use alloy::{rpc::types::Log, sol};

use futures::future::join_all;
use nep141_connector::Nep141Connector;

use crate::{defaults, utils};

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

pub async fn finalize_withdraw(redis_client: redis::Client, connector: Arc<Nep141Connector>) {
    let redis_connection = redis_client
        .get_multiplexed_tokio_connection()
        .await
        .unwrap();

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events_test(
            &mut redis_connection_clone,
            "near_sign_transfer_events".to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                defaults::SLEEP_TIME_AFTER_EVENTS_PROCESS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        while let Some((key, event)) = events.next_item().await {
            if let Ok(event) = serde_json::from_str::<Log>(&event) {
                handlers.push(tokio::spawn({
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
                                    utils::redis::remove_event_test(
                                        &mut redis_connection,
                                        "near_sign_transfer_events",
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
            defaults::SLEEP_TIME_AFTER_EVENTS_PROCESS,
        ))
        .await;
    }
}
