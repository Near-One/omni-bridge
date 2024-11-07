use std::sync::Arc;

use alloy::rpc::types::{Log, TransactionReceipt};
use anyhow::Result;
use ethereum_types::H256;
use futures::future::join_all;
use log::{error, info, warn};

use near_jsonrpc_client::JsonRpcClient;
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
    jsonrpc_client: JsonRpcClient,
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
                    let jsonrpc_client = jsonrpc_client.clone();

                    async move {
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

                        let current_timestamp = chrono::Utc::now().timestamp();

                        if current_timestamp - init_transfer_with_timestamp.creation_timestamp
                            > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR 
                        {
                            warn!(
                                "Removing InitTransfer that is older than 2 weeks: {:?}",
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

                        if current_timestamp
                            - init_transfer_with_timestamp
                                .last_update_timestamp
                                .unwrap_or_default()
                            < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
                        {
                            return;
                        }

                        let is_fee_sufficient = 
                            match utils::fee::is_fee_sufficient(
                                &config,
                                &jsonrpc_client,
                                &transfer_message.sender,
                                &transfer_message.recipient,
                                &transfer_message.token,
                                &transfer_message.fee,
                            )
                            .await
                        {
                            Ok(is_fee_sufficient) => {
                                if !is_fee_sufficient {
                                    warn!("Fee is not sufficient for transfer: {:?}", init_transfer_with_timestamp.event);
                                }

                                is_fee_sufficient
                            }
                            Err(err) => {
                                warn!("Failed to check fee: {}", err);

                                 false
                            }
                        };

                        if !is_fee_sufficient {
                            utils::redis::add_event(
                                &mut redis_connection,
                                utils::redis::NEAR_INIT_TRANSFER_QUEUE,
                                transfer_message.origin_nonce.0.to_string(),
                                InitTransferWithTimestamp {
                                    event: init_transfer_with_timestamp.event,
                                    creation_timestamp: init_transfer_with_timestamp.creation_timestamp,
                                    last_update_timestamp: Some(current_timestamp),
                                },
                            )
                            .await;
                            return;
                        }

                        match connector
                            .sign_transfer(
                                transfer_message.origin_nonce.into(),
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FinTransfer {
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
                let vaa =
                    utils::evm::get_vaa(fin_transfer.tx_logs, &fin_transfer.log, &config).await;

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
                            chain_kind: ChainKind::Eth,
                            prover_args,
                            native_fee_recipient: Some(config.evm.relayer_address_on_eth),
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
                        let (Nep141LockerEvent::FinTransferEvent {
                            ref transfer_message,
                            ..
                        }
                        | Nep141LockerEvent::ClaimFeeEvent {
                            ref transfer_message,
                        }) = event
                        else {
                            warn!("Expected FinTransferEvent/ClaimFeeEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received FinTransferEvent/ClaimFeeEvent log");

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
