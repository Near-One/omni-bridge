use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use log::{error, info, warn};
use solana_sdk::signer::EncodableKey;
use solana_transaction_status::{UiMessage, UiTransactionEncoding};
use tokio_stream::StreamExt;

use omni_types::ChainKind;
use solana_client::nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient};
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_config::{
    RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::signature::{Keypair, Signature};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use crate::{config, utils};

pub fn get_keypair(file: Option<&String>) -> Keypair {
    if let Some(file) = file {
        if let Ok(keypair) = Keypair::read_from_file(file) {
            info!("Retrieved keypair from file");
            return keypair;
        }
    }

    info!("Retrieving Solana keypair from env");

    Keypair::from_base58_string(&config::get_private_key(ChainKind::Sol))
}

pub async fn start_indexer(
    config: config::Config,
    redis_client: redis::Client,
    mut start_signature: Option<Signature>,
) -> Result<()> {
    let Some(solana_config) = config.solana else {
        anyhow::bail!("Failed to get Solana config");
    };

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let rpc_http_url = &solana_config.rpc_http_url;
    let rpc_ws_url = &solana_config.rpc_ws_url;
    let program_id = Pubkey::from_str(&solana_config.program_id)?;

    loop {
        crate::skip_fail!(
            process_recent_signatures(
                &mut redis_connection,
                rpc_http_url.clone(),
                &program_id,
                start_signature,
            )
            .await,
            "Failed to process recent signatures",
            5
        );

        info!("All historical logs processed, starting Solana WS subscription");

        let filter = RpcTransactionLogsFilter::Mentions(vec![program_id.to_string()]);
        let config = RpcTransactionLogsConfig {
            commitment: Some(CommitmentConfig::confirmed()),
        };

        let ws_client = crate::skip_fail!(
            PubsubClient::new(rpc_ws_url).await,
            "Solana WebSocket connection failed",
            5
        );

        let (mut log_stream, _) = crate::skip_fail!(
            ws_client
                .logs_subscribe(filter.clone(), config.clone())
                .await,
            "Subscription to logs on Solana chain failed",
            5
        );

        info!("Subscribed to Solana logs");

        while let Some(log) = log_stream.next().await {
            if let Ok(signature) = Signature::from_str(&log.value.signature) {
                info!("Found a signature on Solana: {signature:?}");
                utils::redis::add_event(
                    &mut redis_connection,
                    utils::redis::SOLANA_EVENTS,
                    signature.to_string(),
                    serde_json::Value::Null,
                )
                .await;
            } else {
                warn!("Failed to parse signature: {:?}", log.value.signature);
            }
        }

        error!("Solana WebSocket stream closed, reconnecting...");
        start_signature = None;

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn process_recent_signatures(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    rpc_http_url: String,
    program_id: &Pubkey,
    start_signature: Option<Signature>,
) -> Result<()> {
    let http_client = RpcClient::new(rpc_http_url);

    let from_signature = if let Some(signature) = start_signature {
        utils::redis::add_event(
            redis_connection,
            utils::redis::SOLANA_EVENTS,
            signature.to_string(),
            // TODO: It's better to come up with a solution that wouldn't require storing `Null` value
            serde_json::Value::Null,
        )
        .await;

        signature
    } else {
        let Some(signature) = utils::redis::get_last_processed::<&str, String>(
            redis_connection,
            &utils::redis::get_last_processed_key(ChainKind::Sol),
        )
        .await
        .and_then(|s| Signature::from_str(&s).ok()) else {
            return Ok(());
        };

        signature
    };

    let signatures: Vec<RpcConfirmedTransactionStatusWithSignature> = http_client
        .get_signatures_for_address_with_config(
            program_id,
            GetConfirmedSignaturesForAddress2Config {
                limit: None,
                before: None,
                until: Some(from_signature),
                commitment: Some(CommitmentConfig::confirmed()),
            },
        )
        .await?;

    for signature_status in &signatures {
        utils::redis::add_event(
            redis_connection,
            utils::redis::SOLANA_EVENTS,
            signature_status.signature.clone(),
            // TODO: It's better to come up with a solution that wouldn't require storing `Null` value
            serde_json::Value::Null,
        )
        .await;
    }

    Ok(())
}

pub async fn process_signature(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let Some(solana_config) = config.solana else {
        anyhow::bail!("Failed to get Solana config");
    };

    let rpc_http_url = &solana_config.rpc_http_url;
    let http_client = Arc::new(RpcClient::new(rpc_http_url.to_string()));

    let fetching_config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection = redis_connection.clone();

        let Some(events) = utils::redis::get_events(
            &mut redis_connection,
            utils::redis::SOLANA_EVENTS.to_string(),
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

        for (key, _) in events {
            handlers.push(tokio::spawn({
                let mut redis_connection = redis_connection.clone();
                let solana = solana_config.clone();
                let http_client = http_client.clone();

                async move {
                    let Ok(signature) = Signature::from_str(&key) else {
                        warn!("Failed to parse signature: {key:?}");
                        return;
                    };

                    info!("Trying to process signature: {signature:?}");

                    match http_client
                        .get_transaction_with_config(&signature, fetching_config)
                        .await
                    {
                        Ok(tx) => {
                            let transaction = tx.transaction;

                            if let solana_transaction_status::EncodedTransaction::Json(ref tx) =
                                transaction.transaction
                            {
                                if let UiMessage::Raw(ref raw) = tx.message {
                                    utils::solana::process_message(
                                        &mut redis_connection,
                                        &solana,
                                        &transaction,
                                        raw,
                                        signature,
                                    )
                                    .await;
                                }
                            }

                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::SOLANA_EVENTS,
                                &signature.to_string(),
                            )
                            .await;
                            utils::redis::update_last_processed(
                                &mut redis_connection,
                                &utils::redis::get_last_processed_key(ChainKind::Sol),
                                &signature.to_string(),
                            )
                            .await;
                        }
                        Err(err) => {
                            warn!("Failed to fetch transaction (probably signature wasn't finalized yet): {err:?}");
                        }
                    }
                }
            }));
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}
