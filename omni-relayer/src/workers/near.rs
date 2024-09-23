use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use log::{error, info, warn};

use alloy::rpc::types::Log;
use ethereum_types::H256;
use near_primitives::borsh::BorshSerialize;
use nep141_connector::Nep141Connector;
use omni_types::{
    locker_args::ClaimFeeArgs, near_events::Nep141LockerEvent, prover_args::EvmVerifyProofArgs,
    prover_result::ProofKind, ChainKind,
};

use crate::{config, utils};

pub async fn sign_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<Nep141Connector>,
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
                                transfer_message.fee.into(),
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
    connector: Arc<Nep141Connector>,
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

                        match connector.finalize_deposit_omni_with_log(event).await {
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
    connector: Arc<Nep141Connector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::FINALISED_TRANSFERS.to_string(),
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
                            H256::from_slice(tx_hash.as_slice()),
                            log_index,
                            &config.eth.rpc_http_url,
                        )
                        .await
                        {
                            Ok(proof) => {
                                let evm_proof_args = EvmVerifyProofArgs {
                                    proof_kind: ProofKind::InitTransfer,
                                    proof,
                                };

                                let mut prover_args = Vec::new();
                                if let Err(err) = evm_proof_args.serialize(&mut prover_args) {
                                    warn!("Failed to serialize evm proof: {}", err);
                                    return;
                                }

                                if let Ok(response) = connector
                                    .claim_fee(ClaimFeeArgs {
                                        chain_kind: ChainKind::Eth,
                                        prover_args,
                                    })
                                    .await
                                {
                                    info!("Claimed fee: {:?}", response);
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::FINALISED_TRANSFERS,
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
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}
