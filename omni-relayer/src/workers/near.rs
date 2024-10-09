use std::sync::Arc;

use alloy::rpc::types::{Log, TransactionReceipt};
use anyhow::Result;
use ethereum_types::H256;
use futures::future::join_all;
use log::{error, info, warn};

use near_jsonrpc_client::JsonRpcClient;
use omni_connector::OmniConnector;
use omni_types::{locker_args::ClaimFeeArgs, near_events::Nep141LockerEvent, ChainKind};

use crate::{config, utils};

pub async fn check_bad_fees(
    redis_client: redis::Client,
    jsonrpc_client: JsonRpcClient,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_BAD_FEE_EVENTS.to_string(),
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
                    let jsonrpc_client = jsonrpc_client.clone();

                    async move {
                        let Nep141LockerEvent::InitTransferEvent {
                            ref transfer_message,
                        } = event
                        else {
                            warn!("Expected InitTransferEvent, got: {:?}", event);
                            return;
                        };

                        info!(
                            "Received InitTransferEvent with bad fee: {}",
                            transfer_message.origin_nonce.0
                        );

                        if matches!(
                            utils::fee::is_fee_sufficient(
                                &jsonrpc_client,
                                &transfer_message.sender,
                                &transfer_message.recipient,
                                &transfer_message.token,
                                transfer_message.fee.fee.into(),
                            )
                            .await,
                            Ok(true)
                        ) {
                            info!("Fee is now sufficient");

                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::NEAR_BAD_FEE_EVENTS,
                                &key,
                            )
                            .await;
                            utils::redis::add_event(
                                &mut redis_connection,
                                utils::redis::NEAR_INIT_TRANSFER_EVENTS,
                                transfer_message.origin_nonce.0.to_string(),
                                event,
                            )
                            .await;
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
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
            utils::redis::NEAR_INIT_TRANSFER_EVENTS.to_string(),
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
                                Some(config.near.token_locker_id),
                                transfer_message.fee.fee.into(),
                            )
                            .await
                        {
                            Ok(outcome) => {
                                info!("Signed transfer: {:?}", outcome.transaction.hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_INIT_TRANSFER_EVENTS,
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
    jsonrpc_client: JsonRpcClient,
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
            if let Ok((block_number, log, tx_logs)) =
                serde_json::from_str::<(u64, Log, Option<TransactionReceipt>)>(&event)
            {
                let Ok(light_client_latest_block_number) =
                    utils::near::get_eth_light_client_last_block_number(&config, &jsonrpc_client)
                        .await
                else {
                    warn!("Failed to get eth light client last block number");
                    continue;
                };

                if block_number > light_client_latest_block_number {
                    continue;
                }

                if log.log_decode::<utils::evm::FinTransfer>().is_err() {
                    warn!("Failed to decode log as FinTransfer: {:?}", log);
                    continue;
                };

                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        info!("Received FinTransfer event");
                        let Some(tx_hash) = log.transaction_hash else {
                            warn!("No transaction hash in log: {:?}", log);
                            return;
                        };

                        let Some(log_index) = log.log_index else {
                            warn!("No log index in log: {:?}", log);
                            return;
                        };

                        let tx_hash = H256::from_slice(tx_hash.as_slice());

                        let vaa = utils::evm::get_vaa(tx_logs, &log, &config).await;

                        let Some(prover_args) =
                            utils::evm::get_prover_args(vaa, tx_hash, log_index, &config).await
                        else {
                            return;
                        };

                        let claim_fee_args = ClaimFeeArgs {
                            chain_kind: ChainKind::Eth,
                            prover_args,
                            native_fee_recipient: config.evm.relayer.clone(),
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
            utils::redis::NEAR_FIN_TRANSFER_EVENTS.to_string(),
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
                        let Nep141LockerEvent::FinTransferEvent {
                            ref transfer_message,
                            ..
                        } = event
                        else {
                            warn!("Expected FinTransferEvent, got: {:?}", event);
                            return;
                        };

                        log::info!("Received FinTransferEvent log");

                        match connector
                            .sign_claim_native_fee(
                                vec![transfer_message.origin_nonce.into()],
                                config.evm.relayer,
                            )
                            .await
                        {
                            Ok(tx_hash) => {
                                log::info!("Claimed native fee: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_FIN_TRANSFER_EVENTS,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => log::error!("Failed to sign claiming native fee: {}", err),
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
