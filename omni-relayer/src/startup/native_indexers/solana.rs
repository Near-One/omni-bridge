use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use borsh::BorshDeserialize;
use futures::future::join_all;
use solana_client::nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient};
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_config::{
    RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::signature::Signature;
use solana_sdk::{bs58, commitment_config::CommitmentConfig, pubkey::Pubkey};
use solana_transaction_status::{
    EncodedTransactionWithStatusMeta, UiMessage, UiRawMessage, UiTransactionEncoding,
    option_serializer::OptionSerializer,
};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

use omni_types::{ChainKind, OmniAddress};

use crate::{config, utils, workers::{DeployToken, FinTransfer, RetryableEvent, Transfer}};

pub async fn start_indexer(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    mut start_signature: Option<Signature>,
) -> Result<()> {
    let Some(solana_config) = config.solana.clone() else {
        anyhow::bail!("Failed to get Solana config");
    };

    let rpc_http_url = &solana_config.rpc_http_url;
    let rpc_ws_url = &solana_config.rpc_ws_url;
    let program_id = Pubkey::from_str(&solana_config.program_id)?;

    loop {
        crate::skip_fail!(
            process_recent_signatures(
                config,
                redis_connection_manager,
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
        let rpc_config = RpcTransactionLogsConfig {
            commitment: Some(CommitmentConfig::confirmed()),
        };

        let ws_client = crate::skip_fail!(
            PubsubClient::new(rpc_ws_url).await,
            "Solana WebSocket connection failed",
            5
        );

        let (mut log_stream, _) = crate::skip_fail!(
            ws_client
                .logs_subscribe(filter.clone(), rpc_config.clone())
                .await,
            "Subscription to logs on Solana chain failed",
            5
        );

        info!("Subscribed to Solana logs");

        while let Some(log) = log_stream.next().await {
            if let Ok(signature) = Signature::from_str(&log.value.signature) {
                info!("Found a signature on Solana: {signature:?}");
                utils::redis::add_event(
                    config,
                    redis_connection_manager,
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
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    rpc_http_url: String,
    program_id: &Pubkey,
    start_signature: Option<Signature>,
) -> Result<()> {
    let http_client = RpcClient::new(rpc_http_url);

    let from_signature = if let Some(signature) = start_signature {
        utils::redis::add_event(
            config,
            redis_connection_manager,
            utils::redis::SOLANA_EVENTS,
            signature.to_string(),
            // TODO: It's better to come up with a solution that wouldn't require storing `Null` value
            serde_json::Value::Null,
        )
        .await;

        signature
    } else {
        let Some(signature) = utils::redis::get_last_processed::<&str, String>(
            config,
            redis_connection_manager,
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
            config,
            redis_connection_manager,
            utils::redis::SOLANA_EVENTS,
            signature_status.signature.clone(),
            // TODO: It's better to come up with a solution that wouldn't require storing `Null` value
            serde_json::Value::Null,
        )
        .await;
    }

    Ok(())
}

pub async fn process_signature(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
) -> Result<()> {
    let Some(solana_config) = config.solana.clone() else {
        anyhow::bail!("Failed to get Solana config");
    };

    let rpc_http_url = &solana_config.rpc_http_url;
    let http_client = Arc::new(RpcClient::new(rpc_http_url.to_string()));

    let fetching_config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    loop {
        let Some(events) = utils::redis::get_events(
            config,
            redis_connection_manager,
            utils::redis::SOLANA_EVENTS.to_string(),
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

        for (key, _) in events {
            handlers.push(tokio::spawn({
                let config = config.clone();
                let mut redis_connection_manager = redis_connection_manager.clone();
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
                                    process_message(
                                        &config,
                                        &mut redis_connection_manager,
                                        &solana,
                                        &transaction,
                                        raw,
                                        signature,
                                    )
                                    .await;
                                }
                            }

                            utils::redis::remove_event(
                                &config,
                                &mut redis_connection_manager,
                                utils::redis::SOLANA_EVENTS,
                                &signature.to_string(),
                            )
                            .await;
                            utils::redis::update_last_processed(
                                &config,
                                &mut redis_connection_manager,
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
            config.redis.sleep_time_after_events_process_secs,
        ))
        .await;
    }
}

#[derive(Debug, BorshDeserialize)]
struct InitTransferPayload {
    pub amount: u128,
    pub recipient: String,
    pub fee: u128,
    pub native_fee: u64,
    pub message: String,
}

async fn process_message(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    solana: &config::Solana,
    transaction: &EncodedTransactionWithStatusMeta,
    message: &UiRawMessage,
    signature: Signature,
) {
    for instruction in message.instructions.clone() {
        let account_keys = instruction
            .accounts
            .into_iter()
            .map(|i| message.account_keys.get(usize::from(i)).cloned())
            .collect::<Vec<_>>();

        if let Err(err) = decode_instruction(
            config,
            redis_connection_manager,
            solana,
            signature,
            transaction,
            &instruction.data,
            account_keys,
        )
        .await
        {
            warn!("Failed to decode instruction: {err:?}");
        }
    }
}

async fn decode_instruction(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    solana: &config::Solana,
    signature: Signature,
    transaction: &EncodedTransactionWithStatusMeta,
    data: &str,
    account_keys: Vec<Option<String>>,
) -> Result<()> {
    let decoded_data = bs58::decode(data).into_vec()?;

    if let Some((discriminator, offset)) = [
        (
            &solana.init_transfer_discriminator,
            solana.init_transfer_discriminator.len(),
        ),
        (
            &solana.init_transfer_sol_discriminator,
            solana.init_transfer_sol_discriminator.len(),
        ),
    ]
    .into_iter()
    .find_map(|(discriminator, len)| {
        decoded_data
            .starts_with(discriminator)
            .then_some((discriminator, len))
    }) {
        info!("Received InitTransfer on Solana ({signature})");

        let mut payload_data = decoded_data
            .get(offset..)
            .context("Decoded data is too short")?;

        if let Ok(payload) = InitTransferPayload::deserialize(&mut payload_data) {
            let (sender, token, emitter) = if discriminator == &solana.init_transfer_discriminator {
                let sender = account_keys
                    .get(solana.init_transfer_sender_index)
                    .context("Missing sender account key")?
                    .as_ref()
                    .context("Sender account key is None")?;
                let token = account_keys
                    .get(solana.init_transfer_token_index)
                    .context("Missing token account key")?
                    .as_ref()
                    .context("Sender account key is None")?;
                let emitter = account_keys
                    .get(solana.init_transfer_emitter_index)
                    .context("Missing emitter account key")?
                    .as_ref()
                    .context("Emitter key is None")?;
                (sender, token, emitter)
            } else {
                let sender = account_keys
                    .get(solana.init_transfer_sol_sender_index)
                    .context("Missing SOL sender account key")?
                    .as_ref()
                    .context("SOL sender account key is None")?;
                let emitter = account_keys
                    .get(solana.init_transfer_sol_emitter_index)
                    .context("Missing SOL emitter account key")?
                    .as_ref()
                    .context("Sol emitter key is None")?;
                (sender, &Pubkey::default().to_string(), emitter)
            };

            if let Some(OptionSerializer::Some(logs)) =
                transaction.clone().meta.map(|meta| meta.log_messages)
            {
                for log in logs {
                    if log.contains("Sequence") {
                        let Ok(Ok(sender)) = Pubkey::from_str(sender).map(|sender| {
                            OmniAddress::new_from_slice(ChainKind::Sol, &sender.to_bytes())
                        }) else {
                            warn!("Failed to parse sender as a pubkey: {sender:?}");
                            continue;
                        };

                        let Ok(recipient) = payload.recipient.parse::<OmniAddress>() else {
                            warn!(
                                "Failed to parse recipient as OmniAddress: {:?}",
                                payload.recipient
                            );
                            continue;
                        };

                        let Ok(token) = Pubkey::from_str(token) else {
                            warn!("Failed to parse token as a pubkey: {token:?}");
                            continue;
                        };

                        let Some(Ok(sequence)) =
                            log.split_ascii_whitespace().last().map(str::parse)
                        else {
                            warn!("Failed to parse sequence number from log: {log:?}");
                            continue;
                        };

                        let Ok(emitter) = Pubkey::from_str(emitter) else {
                            warn!("Failed to parse emitter as a pubkey: {emitter:?}");
                            continue;
                        };

                        utils::redis::add_event(
                            config,
                            redis_connection_manager,
                            utils::redis::EVENTS,
                            signature.to_string(),
                            RetryableEvent::new(Transfer::Solana {
                                amount: payload.amount.into(),
                                token,
                                sender,
                                recipient,
                                fee: payload.fee.into(),
                                native_fee: payload.native_fee,
                                message: payload.message.clone(),
                                emitter,
                                sequence,
                            }),
                        )
                        .await;
                    }
                }
            }
        }
    } else if let Some(discriminator) = [
        &solana.finalize_transfer_discriminator,
        &solana.finalize_transfer_sol_discriminator,
    ]
    .into_iter()
    .find(|discriminator| decoded_data.starts_with(discriminator))
    {
        info!("Received FinTransfer on Solana: {signature}");

        let emitter = if discriminator == &solana.finalize_transfer_discriminator {
            account_keys
                .get(solana.finalize_transfer_emitter_index)
                .context("Missing emitter account key")?
                .as_ref()
                .context("Emitter account key is None")?
        } else {
            account_keys
                .get(solana.finalize_transfer_sol_emitter_index)
                .context("Missing SOL emitter account key")?
                .as_ref()
                .context("SOL emitter account key is None")?
        };

        if let Some(OptionSerializer::Some(logs)) =
            transaction.clone().meta.map(|meta| meta.log_messages)
        {
            for log in logs {
                if log.contains("Sequence") {
                    let Some(sequence) = log
                        .split_ascii_whitespace()
                        .last()
                        .map(std::string::ToString::to_string)
                    else {
                        warn!("Failed to parse sequence number from log: {log:?}");
                        continue;
                    };

                    let Ok(sequence) = sequence.parse() else {
                        warn!("Failed to parse sequence as a number: {sequence:?}");
                        continue;
                    };

                    utils::redis::add_event(
                        config,
                        redis_connection_manager,
                        utils::redis::EVENTS,
                        signature.to_string(),
                        RetryableEvent::new(FinTransfer::Solana {
                            emitter: emitter.clone(),
                            sequence,
                            transfer_id: None,
                        }),
                    )
                    .await;
                }
            }
        }
    } else if decoded_data.starts_with(&solana.deploy_token_discriminator) {
        info!("Received DeployToken on Solana ({signature})");

        if let Some(OptionSerializer::Some(logs)) =
            transaction.clone().meta.map(|meta| meta.log_messages)
        {
            for log in logs {
                if log.contains("Sequence") {
                    let Some(sequence) = log
                        .split_ascii_whitespace()
                        .last()
                        .map(std::string::ToString::to_string)
                    else {
                        warn!("Failed to parse sequence number from log: {log:?}");
                        continue;
                    };
                    let Ok(sequence) = sequence.parse() else {
                        warn!("Failed to parse sequence as a number: {sequence:?}");
                        continue;
                    };

                    utils::redis::add_event(
                        config,
                        redis_connection_manager,
                        utils::redis::EVENTS,
                        signature.to_string(),
                        RetryableEvent::new(DeployToken::Solana {
                            emitter: account_keys
                                .get(solana.deploy_token_emitter_index)
                                .context("Missing emitter account key")?
                                .as_ref()
                                .context("Emitter account key is None")?
                                .clone(),
                            sequence,
                        }),
                    )
                    .await;
                }
            }
        }
    }

    Ok(())
}
