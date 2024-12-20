use std::str::FromStr;

use anyhow::Result;
use log::{info, warn};
use solana_transaction_status::option_serializer::OptionSerializer;
use tokio_stream::StreamExt;

use anchor_lang::prelude::borsh;
use anchor_lang::AnchorDeserialize;
use omni_types::ChainKind;
use solana_client::nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient};
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::bs58;
use solana_sdk::signature::Signature;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use solana_transaction_status::{
    EncodedTransactionWithStatusMeta, UiMessage, UiRawMessage, UiTransactionEncoding,
};

use crate::workers::near::FinTransfer;
use crate::workers::solana::InitTransferWithTimestamp;
use crate::{config, utils};

#[derive(Debug, AnchorDeserialize)]
struct InitTransferPayload {
    pub amount: u128,
    pub recipient: String,
    pub fee: u128,
    pub native_fee: u64,
}

pub async fn start_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let Some(solana) = config.solana else {
        anyhow::bail!("Failed to get Solana config");
    };

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let rpc_http_url = &solana.rpc_http_url;
    let rpc_ws_url = &solana.rpc_ws_url;
    let program_id = Pubkey::from_str(&solana.program_id)?;

    let http_client = RpcClient::new(rpc_http_url.to_string());

    if let Err(e) =
        process_recent_logs(&mut redis_connection, &solana, &http_client, &program_id).await
    {
        warn!("Failed to fetch recent logs: {}", e);
    }

    info!("All historical logs processed, starting Solana WS subscription");

    let Ok(ws_client) = PubsubClient::new(rpc_ws_url).await else {
        anyhow::bail!("Failed to connect to Solana WebSocket");
    };

    let filter = RpcTransactionLogsFilter::Mentions(vec![program_id.to_string()]);
    let config = RpcTransactionLogsConfig {
        commitment: Some(CommitmentConfig::processed()),
    };

    let Ok((mut log_stream, _)) = ws_client.logs_subscribe(filter, config).await else {
        anyhow::bail!("Failed to subscribe to Solana logs");
    };

    info!("Subscribed to live Solana logs");

    while let Some(log) = log_stream.next().await {
        let Ok(signature) = Signature::from_str(&log.value.signature) else {
            warn!("Failed to parse signature: {:?}", log.value.signature);
            continue;
        };

        info!("Processing signature: {:?}", signature);

        // TODO: We need to wait for the transaction to be confirmed, but this must be replaced
        // with a worker
        tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

        match http_client
            .get_transaction(&signature, UiTransactionEncoding::Json)
            .await
        {
            Ok(tx) => {
                let transaction = tx.transaction;

                if let solana_transaction_status::EncodedTransaction::Json(ref tx) =
                    transaction.transaction
                {
                    if let UiMessage::Raw(ref raw) = tx.message {
                        process_message(
                            &mut redis_connection,
                            &solana,
                            &transaction,
                            raw,
                            signature,
                        )
                        .await
                    }
                }

                utils::redis::update_last_processed_block(
                    &mut redis_connection,
                    &utils::redis::get_last_processed_block_key(ChainKind::Sol).await,
                    tx.slot,
                )
                .await;
            }
            Err(e) => {
                warn!("Failed to fetch transaction: {}", e);
            }
        };
    }

    Ok(())
}

async fn process_recent_logs(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    solana: &config::Solana,
    http_client: &RpcClient,
    program_id: &Pubkey,
) -> Result<()> {
    let Some(start_block_height) = utils::redis::get_last_processed_block(
        redis_connection,
        &utils::redis::get_last_processed_block_key(ChainKind::Sol).await,
    )
    .await
    else {
        return Ok(());
    };

    // TODO: Replace all this simply by saving last signature instead of slot
    let mut last_signature = http_client
        .get_signatures_for_address_with_config(
            program_id,
            GetConfirmedSignaturesForAddress2Config {
                limit: Some(1000),
                before: None,
                until: None,
                commitment: Some(CommitmentConfig::confirmed()),
            },
        )
        .await?
        .into_iter()
        .find(|sig| sig.slot == start_block_height)
        .and_then(|sig| Signature::from_str(&sig.signature).ok());

    if last_signature.is_none() {
        return Ok(());
    }

    loop {
        let signatures: Vec<RpcConfirmedTransactionStatusWithSignature> = http_client
            .get_signatures_for_address_with_config(
                program_id,
                GetConfirmedSignaturesForAddress2Config {
                    limit: None,
                    before: last_signature,
                    until: None,
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        if signatures.is_empty() {
            break;
        }

        for signature_status in &signatures {
            let signature = Signature::from_str(&signature_status.signature)?;

            if let Ok(tx) = http_client
                .get_transaction(&signature, UiTransactionEncoding::Json)
                .await
            {
                let transaction = tx.transaction;

                if let solana_transaction_status::EncodedTransaction::Json(ref tx) =
                    transaction.transaction
                {
                    if let UiMessage::Raw(ref raw) = tx.message {
                        process_message(redis_connection, solana, &transaction, raw, signature)
                            .await
                    }
                }
            }

            utils::redis::update_last_processed_block(
                redis_connection,
                &utils::redis::get_last_processed_block_key(ChainKind::Sol).await,
                signature_status.slot,
            )
            .await;
        }

        last_signature = signatures
            .last()
            .and_then(|s| Signature::from_str(&s.signature).ok());
    }

    Ok(())
}

async fn process_message(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    solana: &config::Solana,
    transaction: &EncodedTransactionWithStatusMeta,
    message: &UiRawMessage,
    signature: Signature,
) {
    for instruction in message.instructions.clone() {
        if let Err(err) = decode_instruction(
            redis_connection,
            solana,
            signature,
            transaction,
            &instruction.data,
            &message.account_keys,
        )
        .await
        {
            warn!("Failed to decode instruction: {}", err);
        }
    }
}

async fn decode_instruction(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    solana: &config::Solana,
    signature: Signature,
    transaction: &EncodedTransactionWithStatusMeta,
    data: &str,
    account_keys: &[String],
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
        info!("Received InitTransfer on Solana");

        let mut payload_data = &decoded_data[offset..];

        if let Ok(payload) = InitTransferPayload::deserialize(&mut payload_data) {
            let (token, emitter) = if discriminator == &solana.init_transfer_discriminator {
                (
                    &account_keys[solana.init_transfer_token_index],
                    &account_keys[solana.init_transfer_emitter_index],
                )
            } else {
                (
                    &Pubkey::default().to_string(),
                    &account_keys[solana.init_transfer_sol_emitter_index],
                )
            };

            if let Some(OptionSerializer::Some(logs)) =
                transaction.clone().meta.map(|meta| meta.log_messages)
            {
                for log in logs {
                    if log.contains("Sequence") {
                        let Some(sequence) = log
                            .split_ascii_whitespace()
                            .last()
                            .map(|sequence| sequence.to_string())
                        else {
                            warn!("Failed to parse sequence number from log: {:?}", log);
                            continue;
                        };

                        let Ok(sequence) = sequence.parse() else {
                            warn!("Failed to parse sequence as a number: {:?}", sequence);
                            continue;
                        };

                        utils::redis::add_event(
                            redis_connection,
                            utils::redis::SOLANA_INIT_TRANSFER_EVENTS,
                            signature.to_string(),
                            InitTransferWithTimestamp {
                                amount: payload.amount,
                                token: token.clone(),
                                recipient: payload.recipient.clone(),
                                fee: payload.fee,
                                native_fee: payload.native_fee,
                                emitter: emitter.clone(),
                                sequence,
                                creation_timestamp: chrono::Utc::now().timestamp(),
                                last_update_timestamp: None,
                            },
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
        info!("Received FinTransfer on Solana");

        let emitter = if discriminator == &solana.finalize_transfer_discriminator {
            &account_keys[solana.finalize_transfer_emitter_index]
        } else {
            &account_keys[solana.finalize_transfer_sol_emitter_index]
        };

        if let Some(OptionSerializer::Some(logs)) =
            transaction.clone().meta.map(|meta| meta.log_messages)
        {
            for log in logs {
                if log.contains("Sequence") {
                    let Some(sequence) = log
                        .split_ascii_whitespace()
                        .last()
                        .map(|sequence| sequence.to_string())
                    else {
                        warn!("Failed to parse sequence number from log: {:?}", log);
                        continue;
                    };

                    let Ok(sequence) = sequence.parse() else {
                        warn!("Failed to parse sequence as a number: {:?}", sequence);
                        continue;
                    };

                    utils::redis::add_event(
                        redis_connection,
                        utils::redis::FINALIZED_TRANSFERS,
                        signature.to_string(),
                        FinTransfer::Solana {
                            emitter: emitter.clone(),
                            sequence,
                        },
                    )
                    .await;
                }
            }
        }
    }

    Ok(())
}
