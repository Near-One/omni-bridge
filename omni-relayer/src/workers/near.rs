use std::sync::Arc;

use alloy::rpc::types::Log;
use anyhow::Result;
use futures::future::join_all;
use log::{error, info, warn};

use near_primitives::borsh::BorshSerialize;
use nep141_connector::Nep141Connector;
use omni_types::{locker_args::ClaimFeeArgs, near_events::Nep141LockerEvent, ChainKind};

use crate::{config, utils};

pub async fn sign_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<Nep141Connector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

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
                                info!("Signed transfer: {:?}", outcome.transaction.hash);
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
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

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

pub async fn claim_fee(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<Nep141Connector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events(
            &mut redis_connection_clone,
            config.redis.eth_deposit_events.clone(),
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
            if let Ok(deposit_log) =
                serde_json::from_str::<Log<crate::startup::eth::Deposit>>(&event)
            {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        info!("Decoded log: {:?}", deposit_log);

                        let Some(tx_hash) = deposit_log.transaction_hash else {
                            log::warn!("No transaction hash in log: {:?}", deposit_log);
                            return;
                        };
                        let Some(log_index) = deposit_log.log_index else {
                            log::warn!("No log index in log: {:?}", deposit_log);
                            return;
                        };

                        match eth_proof::get_proof_for_event(
                            primitive_types::H256::from_slice(tx_hash.as_slice()),
                            log_index,
                            &config.mainnet.eth_rpc_http_url,
                        )
                        .await
                        {
                            Ok(proof) => {
                                let mut args = Vec::new();
                                if proof.serialize(&mut args).is_err() {
                                    warn!("Failed to serialize proof");
                                    return;
                                }

                                if let Ok(response) = connector
                                    .claim_fee(ClaimFeeArgs {
                                        chain_kind: ChainKind::Eth,
                                        prover_args: args,
                                    })
                                    .await
                                {
                                    info!("Claimed fee: {:?}", response);
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        &config.redis.eth_deposit_events,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                            Err(err) => {
                                error!("Failed to get proof: {}", err);
                            }
                        };
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
