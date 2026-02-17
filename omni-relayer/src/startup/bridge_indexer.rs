use std::str::FromStr;
use std::sync::Arc;

use alloy::primitives::{Address, TxHash};
use anyhow::{Context, Result};
use async_nats::jetstream::consumer::PullConsumer;
use bridge_indexer_types::documents_types::{
    OmniEvent, OmniEventData, OmniMetaEvent, OmniMetaEventDetails, OmniTransactionEvent,
    OmniTransactionOrigin, OmniTransferMessage,
};
use mongodb::{Client, Collection, change_stream::event::ResumeToken, options::ClientOptions};
use omni_types::{
    ChainKind, Fee, OmniAddress, TransferId, TransferIdKind, UnifiedTransferId,
    near_events::OmniBridgeEvent,
};
use solana_sdk::pubkey::Pubkey;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::{
    config::{self},
    utils,
    workers::{self, RetryableEvent},
};

const OMNI_EVENTS: &str = "omni_events";

fn near_event_key(origin_transaction_id: &str, origin_nonce: u64) -> String {
    utils::redis::composite_key(&[origin_transaction_id, &origin_nonce.to_string()])
}

fn evm_event_key(origin_transaction_id: &str, log_index: Option<u64>) -> String {
    let log_index = log_index.unwrap_or_default().to_string();
    utils::redis::composite_key(&[origin_transaction_id, &log_index])
}

fn solana_event_key(origin_transaction_id: &str, instruction_index: Option<usize>) -> String {
    let instruction_index = instruction_index.unwrap_or_default().to_string();
    utils::redis::composite_key(&[origin_transaction_id, &instruction_index])
}

fn near_to_utxo_event_key(origin_transaction_id: &str, utxo_id: &str, sign_index: u64) -> String {
    utils::redis::composite_key(&[origin_transaction_id, utxo_id, &sign_index.to_string()])
}

fn get_evm_config(config: &config::Config, chain_kind: ChainKind) -> Result<&config::Evm> {
    match chain_kind {
        ChainKind::Eth => config.eth.as_ref().context("EVM config for Eth is not set"),
        ChainKind::Base => config
            .base
            .as_ref()
            .context("EVM config for Base is not set"),
        ChainKind::Arb => config.arb.as_ref().context("EVM config for Arb is not set"),
        ChainKind::Bnb => config.bnb.as_ref().context("EVM config for Bnb is not set"),
        ChainKind::Pol => config.pol.as_ref().context("EVM config for Pol is not set"),
        ChainKind::Near | ChainKind::Sol | ChainKind::Btc | ChainKind::Zcash => {
            anyhow::bail!("Unsupported chain kind for EVM: {chain_kind:?}")
        }
    }
}

async fn add_event<E: serde::Serialize + std::fmt::Debug + Sync>(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats: Option<&utils::nats::NatsClient>,
    key: &str,
    event: E,
) {
    if let Some(nats_client) = nats {
        if let (Some(nats_config), Ok(payload)) = (config.nats.as_ref(), serde_json::to_vec(&event))
        {
            let subject = format!("{}.item", nats_config.work_subject);
            nats_client.publish(subject, key, &payload).await;
        }
    }

    let retryable = RetryableEvent::new(event);
    utils::redis::add_event(
        config,
        redis_connection_manager,
        utils::redis::EVENTS,
        key,
        &retryable,
    )
    .await;
}

#[allow(clippy::too_many_lines)]
async fn handle_transaction_event(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats: Option<&utils::nats::NatsClient>,
    origin_transaction_id: String,
    unified_transfer_id: UnifiedTransferId,
    origin: OmniTransactionOrigin,
    event: OmniTransactionEvent,
) -> Result<()> {
    match event.transfer_message {
        OmniTransferMessage::NearTransferMessage(transfer_message) => {
            info!(
                "Received NearTransferMessage ({:?}:{}): {origin_transaction_id}",
                transfer_message.get_origin_chain(),
                transfer_message.origin_nonce
            );

            if transfer_message.recipient.get_chain() != ChainKind::Near {
                let key = near_event_key(&origin_transaction_id, transfer_message.origin_nonce);

                add_event(
                    config,
                    redis_connection_manager,
                    nats,
                    &key,
                    crate::workers::Transfer::Near { transfer_message },
                )
                .await;
            }
        }
        OmniTransferMessage::NearUtxoTransferMessage {
            utxo_transfer_message,
            new_transfer_id,
            ..
        } => {
            info!("Received NearUtxoTransferMessage: {:?}", event.transfer_id);

            if let Some(new_transfer_id) = new_transfer_id {
                let utxo_key = utils::redis::composite_key(&[
                    &origin_transaction_id,
                    &utxo_transfer_message.utxo_id.to_string(),
                ]);

                add_event(
                    config,
                    redis_connection_manager,
                    nats,
                    &utxo_key,
                    crate::workers::Transfer::Utxo {
                        utxo_transfer_message,
                        new_transfer_id,
                    },
                )
                .await;
            }
        }
        OmniTransferMessage::NearSignTransferEvent(sign_event) => {
            info!(
                "Received NearSignTransferEvent ({:?}:{}): {origin_transaction_id}",
                sign_event.message_payload.transfer_id.origin_chain,
                sign_event.message_payload.transfer_id.origin_nonce
            );
            let origin_nonce = sign_event.message_payload.transfer_id.origin_nonce;
            let key = near_event_key(&origin_transaction_id, origin_nonce);

            add_event(
                config,
                redis_connection_manager,
                nats,
                &key,
                OmniBridgeEvent::SignTransferEvent {
                    signature: sign_event.signature,
                    message_payload: sign_event.message_payload,
                },
            )
            .await;
        }
        OmniTransferMessage::NearClaimFeeEvent(_) => {}
        OmniTransferMessage::EvmInitTransferMessage(init_transfer) => {
            let OmniTransactionOrigin::EVMLog {
                block_number,
                block_timestamp,
                chain_kind,
                log_index,
                ..
            } = origin
            else {
                anyhow::bail!("Expected EVMLog for EvmInitTransfer: {init_transfer:?}");
            };

            info!(
                "Received EvmInitTransferMessage ({chain_kind:?}:{}): {origin_transaction_id}",
                init_transfer.origin_nonce
            );

            let log_index_str = log_index.unwrap_or_default().to_string();
            let redis_key = evm_event_key(&origin_transaction_id, log_index);

            let Ok(tx_hash) = TxHash::from_str(&origin_transaction_id) else {
                anyhow::bail!("Failed to parse transaction_id as H256: {origin_transaction_id:?}");
            };

            let (OmniAddress::Eth(sender)
            | OmniAddress::Base(sender)
            | OmniAddress::Arb(sender)
            | OmniAddress::Bnb(sender)
            | OmniAddress::Pol(sender)) = init_transfer.sender.clone()
            else {
                anyhow::bail!("Unexpected token address: {}", init_transfer.sender);
            };

            let (OmniAddress::Eth(token)
            | OmniAddress::Base(token)
            | OmniAddress::Arb(token)
            | OmniAddress::Bnb(token)
            | OmniAddress::Pol(token)) = init_transfer.token.clone()
            else {
                anyhow::bail!("Unexpected token address: {}", init_transfer.token);
            };

            let log = utils::evm::InitTransferMessage {
                sender: Address(sender.0.into()),
                token_address: Address(token.0.into()),
                origin_nonce: init_transfer.origin_nonce,
                amount: init_transfer.amount,
                fee: init_transfer.fee.fee,
                native_fee: init_transfer.fee.native_fee,
                recipient: init_transfer.recipient,
                message: init_transfer.msg,
            };

            let Ok(creation_timestamp) = i64::try_from(block_timestamp) else {
                anyhow::bail!("Failed to parse block_timestamp as i64: {block_timestamp}");
            };

            let expected_finalization_time = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.expected_finalization_time)?;

            let safe_confirmations = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.safe_confirmations)?;

            add_event(
                config,
                redis_connection_manager,
                nats,
                &redis_key,
                workers::Transfer::Evm {
                    chain_kind,
                    tx_hash,
                    log: log.clone(),
                    creation_timestamp,
                    expected_finalization_time,
                },
            )
            .await;

            if config.is_fast_relayer_enabled() {
                let fast_key =
                    utils::redis::composite_key(&["fast", &origin_transaction_id, &log_index_str]);

                add_event(
                    config,
                    redis_connection_manager,
                    nats,
                    &fast_key,
                    crate::workers::Transfer::Fast {
                        block_number,
                        tx_hash: origin_transaction_id,
                        token: log.token_address.to_string(),
                        amount: log.amount,
                        transfer_id: TransferId {
                            origin_chain: chain_kind,
                            origin_nonce: log.origin_nonce,
                        },
                        recipient: log.recipient,
                        fee: Fee {
                            fee: log.fee,
                            native_fee: log.native_fee,
                        },
                        msg: log.message,
                        storage_deposit_amount: None,
                        safe_confirmations,
                    },
                )
                .await;
            }
        }
        OmniTransferMessage::EvmFinTransferMessage(fin_transfer) => {
            let OmniTransactionOrigin::EVMLog {
                block_timestamp,
                chain_kind,
                log_index,
                ..
            } = origin
            else {
                anyhow::bail!("Expected EVMLog for EvmFinTransfer: {fin_transfer:?}");
            };

            info!("Received EvmFinTransferMessage ({chain_kind:?}): {origin_transaction_id}");

            let redis_key = evm_event_key(&origin_transaction_id, log_index);

            let Ok(tx_hash) = TxHash::from_str(&origin_transaction_id) else {
                anyhow::bail!("Failed to parse transaction_id as H256: {origin_transaction_id:?}");
            };

            let Ok(creation_timestamp) = i64::try_from(block_timestamp) else {
                anyhow::bail!("Failed to parse block_timestamp as i64: {block_timestamp}");
            };

            let expected_finalization_time = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.expected_finalization_time)?;

            add_event(
                config,
                redis_connection_manager,
                nats,
                &redis_key,
                workers::FinTransfer::Evm {
                    chain_kind,
                    tx_hash,
                    creation_timestamp,
                    expected_finalization_time,
                    transfer_id: fin_transfer.transfer_id,
                },
            )
            .await;
        }
        OmniTransferMessage::SolanaInitTransfer(init_transfer) => {
            let OmniTransactionOrigin::SolanaTransaction {
                instruction_index, ..
            } = origin
            else {
                anyhow::bail!(
                    "Expected SolanaTransaction for SolanaInitTransfer: {init_transfer:?}"
                );
            };

            info!(
                "Received SolanaInitTransfer ({:?}:{}): {origin_transaction_id}",
                ChainKind::Sol,
                init_transfer.origin_nonce
            );

            let OmniAddress::Sol(ref token) = init_transfer.token else {
                anyhow::bail!("Unexpected token address: {}", init_transfer.token);
            };
            let Ok(native_fee) = u64::try_from(init_transfer.fee.native_fee.0) else {
                anyhow::bail!("Failed to parse native fee for Solana transfer: {init_transfer:?}");
            };
            let Some(emitter) = init_transfer.emitter else {
                anyhow::bail!("Emitter is not set for Solana transfer: {init_transfer:?}");
            };
            let redis_key = solana_event_key(&origin_transaction_id, Some(instruction_index));

            add_event(
                config,
                redis_connection_manager,
                nats,
                &redis_key,
                crate::workers::Transfer::Solana {
                    amount: init_transfer.amount.0.into(),
                    token: Pubkey::new_from_array(token.0),
                    sender: init_transfer.sender,
                    recipient: init_transfer.recipient,
                    fee: init_transfer.fee.fee,
                    native_fee,
                    message: init_transfer.message.unwrap_or_default(),
                    emitter: Pubkey::from_str(&emitter).context("Failed to parse emitter")?,
                    sequence: init_transfer.origin_nonce,
                },
            )
            .await;
        }
        OmniTransferMessage::SolanaFinTransfer(fin_transfer) => {
            let OmniTransactionOrigin::SolanaTransaction {
                instruction_index, ..
            } = origin
            else {
                anyhow::bail!("Expected SolanaTransaction for SolanaFinTransfer: {fin_transfer:?}");
            };

            let Some(emitter) = fin_transfer.emitter.clone() else {
                anyhow::bail!("Emitter is not set for Solana transfer: {fin_transfer:?}");
            };
            let Some(sequence) = fin_transfer.sequence else {
                anyhow::bail!("Sequence is not set for Solana transfer: {fin_transfer:?}");
            };

            info!(
                "Received SolanaFinTransfer ({:?}:{sequence}): {origin_transaction_id}",
                ChainKind::Sol
            );
            let redis_key = solana_event_key(&origin_transaction_id, Some(instruction_index));

            add_event(
                config,
                redis_connection_manager,
                nats,
                &redis_key,
                crate::workers::FinTransfer::Solana {
                    emitter,
                    sequence,
                    transfer_id: (&unified_transfer_id).try_into().ok(),
                },
            )
            .await;
        }
        OmniTransferMessage::UtxoSignTransaction {
            destination_chain,
            relayer,
        } => {
            info!(
                "Received UtxoSignBtcTransaction on {:?}: {origin_transaction_id}",
                event.transfer_id.origin_chain
            );
            add_event(
                config,
                redis_connection_manager,
                nats,
                &origin_transaction_id,
                workers::utxo::SignUtxoTransaction {
                    chain: destination_chain,
                    near_tx_hash: origin_transaction_id.clone(),
                    relayer,
                },
            )
            .await;
        }
        OmniTransferMessage::TransferNearToUtxo {
            destination_chain,
            utxo_count,
            ref new_transfer_id,
            ..
        } => {
            let utxo_id = if let TransferIdKind::Utxo(utxo_id) = event.transfer_id.kind {
                utxo_id
            } else if let Some(TransferIdKind::Utxo(utxo_id)) =
                new_transfer_id.clone().map(|transfer_id| transfer_id.kind)
            {
                utxo_id
            } else {
                anyhow::bail!("Expected Utxo ChainTransferId for TransferNearToUtxo: {event:?}");
            };

            if config.is_signing_utxo_transaction_enabled(destination_chain) {
                info!(
                    "Received TransferNearToUtxo from {:?} to {destination_chain:?}: {origin_transaction_id}",
                    utxo_id.tx_hash
                );

                let utxo_id_str = utxo_id.to_string();

                for sign_index in 0..utxo_count {
                    info!(
                        "Received sign index {sign_index} for BTC pending ID: {}",
                        utxo_id.tx_hash
                    );

                    let redis_key =
                        near_to_utxo_event_key(&origin_transaction_id, &utxo_id_str, sign_index);

                    add_event(
                        config,
                        redis_connection_manager,
                        nats,
                        &redis_key,
                        workers::Transfer::NearToUtxo {
                            chain: destination_chain,
                            btc_pending_id: utxo_id.tx_hash.clone(),
                            sign_index,
                        },
                    )
                    .await;
                }
            }
        }
        OmniTransferMessage::TransferUtxoToNear { ref deposit_msg } => {
            let TransferIdKind::Utxo(utxo_id) = event.transfer_id.kind else {
                anyhow::bail!("Expected Utxo ChainTransferId for TransferUtxoToNear: {event:?}");
            };

            info!(
                "Received TransferUtxoToNear on {:?}: {utxo_id}",
                event.transfer_id.origin_chain
            );
            let key = utxo_id.to_string();
            add_event(
                config,
                redis_connection_manager,
                nats,
                &key,
                workers::Transfer::UtxoToNear {
                    chain: event.transfer_id.origin_chain,
                    btc_tx_hash: utxo_id.tx_hash,
                    vout: utxo_id.vout,
                    deposit_msg: deposit_msg.clone(),
                },
            )
            .await;
        }
        OmniTransferMessage::UtxoConfirmedTxHash { destination_chain } => {
            if config.is_verifying_utxo_withdraw_enabled(destination_chain) {
                let TransferIdKind::Utxo(utxo_id) = event.transfer_id.kind else {
                    anyhow::bail!("Expected Utxo ChainTransferId for ConfirmedTxHash: {event:?}");
                };

                info!(
                    "Received UtxoConfirmedTxHash on {:?}: {utxo_id}",
                    destination_chain
                );
                let key = utxo_id.to_string();
                add_event(
                    config,
                    redis_connection_manager,
                    nats,
                    &key,
                    workers::utxo::ConfirmedTxHash {
                        chain: destination_chain,
                        btc_tx_hash: utxo_id.tx_hash,
                    },
                )
                .await;
            }
        }
        OmniTransferMessage::NearFastTransferMessage { .. } => {
            info!("Received NearFastTransferMessage, skipping");
        }
        OmniTransferMessage::NearFailedTransferMessage { .. } => {
            info!("Received NearFailedTransferMessage, skipping");
        }
        OmniTransferMessage::UtxoVerifyDeposit { .. } => {
            info!("Received UtxoVerifyDeposit, skipping");
        }
        OmniTransferMessage::UtxoVerifyWithdraw { .. } => {
            info!("Received UtxoVerifyWithdraw, skipping");
        }
    }

    Ok(())
}

async fn handle_meta_event(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats: Option<&utils::nats::NatsClient>,
    origin_transaction_id: String,
    origin: OmniTransactionOrigin,
    event: OmniMetaEvent,
) -> Result<()> {
    match event.details {
        OmniMetaEventDetails::EVMDeployToken(deploy_token_event) => {
            let OmniTransactionOrigin::EVMLog {
                block_timestamp,
                chain_kind,
                log_index,
                ..
            } = origin
            else {
                anyhow::bail!("Expected EVMLog for EvmDeployToken: {deploy_token_event:?}");
            };

            info!("Received EVMDeployToken: {origin_transaction_id}");

            let redis_key = evm_event_key(&origin_transaction_id, log_index);

            let Ok(tx_hash) = TxHash::from_str(&origin_transaction_id) else {
                anyhow::bail!("Failed to parse transaction_id as H256: {origin_transaction_id:?}");
            };

            let Ok(creation_timestamp) = i64::try_from(block_timestamp) else {
                anyhow::bail!("Failed to parse block_timestamp as i64: {block_timestamp}");
            };

            let expected_finalization_time = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.expected_finalization_time)?;

            add_event(
                config,
                redis_connection_manager,
                nats,
                &redis_key,
                workers::DeployToken::Evm {
                    chain_kind,
                    tx_hash,
                    creation_timestamp,
                    expected_finalization_time,
                },
            )
            .await;
        }
        OmniMetaEventDetails::SolanaDeployToken {
            emitter, sequence, ..
        } => {
            let OmniTransactionOrigin::SolanaTransaction {
                instruction_index, ..
            } = origin
            else {
                anyhow::bail!("Expected SolanaTransaction for SolanaDeployToken");
            };

            info!("Received SolanaDeployToken: {sequence}");

            let redis_key = solana_event_key(&origin_transaction_id, Some(instruction_index));

            add_event(
                config,
                redis_connection_manager,
                nats,
                &redis_key,
                workers::DeployToken::Solana { emitter, sequence },
            )
            .await;
        }
        OmniMetaEventDetails::EVMLogMetadata(_)
        | OmniMetaEventDetails::EVMOnNearEvent { .. }
        | OmniMetaEventDetails::EVMOnNearInternalTransaction { .. }
        | OmniMetaEventDetails::SolanaLogMetadata { .. }
        | OmniMetaEventDetails::NearLogMetadataEvent { .. }
        | OmniMetaEventDetails::NearDeployTokenEvent { .. }
        | OmniMetaEventDetails::NearBindTokenEvent { .. }
        | OmniMetaEventDetails::NearMigrateTokenEvent { .. }
        | OmniMetaEventDetails::UtxoLogDepositAddress(_) => {}
    }

    Ok(())
}

async fn watch_omni_events_collection(
    collection: &Collection<OmniEvent>,
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    start_timestamp: Option<u32>,
) -> Result<()> {
    let mut stream = if let Some(time) = start_timestamp {
        info!("Starting from timestamp: {time}");

        collection
            .watch()
            .start_at_operation_time(mongodb::bson::Timestamp { time, increment: 0 })
            .await?
    } else {
        let resume_token: Option<ResumeToken> = utils::redis::get_last_processed::<&str, String>(
            config,
            redis_connection_manager,
            utils::redis::MONGODB_OMNI_EVENTS_RT,
        )
        .await
        .and_then(|rt| serde_json::from_str(&rt).ok())
        .unwrap_or_default();

        info!("Resuming from token: {resume_token:?}");

        collection.watch().resume_after(resume_token).await?
    };

    while let Some(change) = stream.next().await {
        match change {
            Ok(doc) => {
                if let Some(event) = doc.full_document {
                    match event.event {
                        OmniEventData::Transaction(transaction_event) => {
                            tokio::spawn({
                                let mut redis_connection_manager = redis_connection_manager.clone();
                                let config = config.clone();

                                async move {
                                    if let Err(err) = handle_transaction_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        None,
                                        event.transaction_id,
                                        transaction_event.transfer_id.clone(),
                                        event.origin,
                                        transaction_event,
                                    )
                                    .await
                                    {
                                        warn!("Failed to handle transaction event: {err:?}");
                                    }
                                }
                            });
                        }
                        OmniEventData::Meta(meta_event) => {
                            tokio::spawn({
                                let mut redis_connection_manager = redis_connection_manager.clone();
                                let config = config.clone();

                                async move {
                                    if let Err(err) = handle_meta_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        None,
                                        event.transaction_id,
                                        event.origin,
                                        meta_event,
                                    )
                                    .await
                                    {
                                        warn!("Failed to handle meta event: {err:?}");
                                    }
                                }
                            });
                        }
                    }
                }
            }
            Err(err) => warn!("Error watching changes: {err:?}"),
        }

        if let Some(ref resume_token) = stream
            .resume_token()
            .and_then(|rt| serde_json::to_string(&rt).ok())
        {
            utils::redis::update_last_processed(
                config,
                redis_connection_manager,
                utils::redis::MONGODB_OMNI_EVENTS_RT,
                resume_token,
            )
            .await;
        }
    }

    Ok(())
}

pub async fn start_indexer(
    config: config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    start_timestamp: Option<u32>,
) -> Result<()> {
    info!("Connecting to bridge-indexer");

    let Some(ref uri) = config.bridge_indexer.mongodb_uri else {
        anyhow::bail!("MONGODB_URI is not set");
    };
    let Some(ref db_name) = config.bridge_indexer.db_name else {
        anyhow::bail!("DB_NAME is not set");
    };

    let client_options = ClientOptions::parse(uri).await?;
    let client = Client::with_options(client_options)?;

    let db = client.database(db_name);
    let omni_events_collection = db.collection::<OmniEvent>(OMNI_EVENTS);

    loop {
        info!("Starting a mongodb stream that track changes in {OMNI_EVENTS}");

        if let Err(err) = watch_omni_events_collection(
            &omni_events_collection,
            &config,
            redis_connection_manager,
            start_timestamp,
        )
        .await
        {
            warn!("Error watching changes: {err:?}");
        }

        warn!("Mongodb stream was closed, restarting...");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

async fn process_nats_message(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats: Option<&utils::nats::NatsClient>,
    event: OmniEvent,
) {
    match event.event {
        OmniEventData::Transaction(transaction_event) => {
            if let Err(err) = handle_transaction_event(
                config,
                redis_connection_manager,
                nats,
                event.transaction_id,
                transaction_event.transfer_id.clone(),
                event.origin,
                transaction_event,
            )
            .await
            {
                warn!("Failed to handle transaction event: {err:?}");
            }
        }
        OmniEventData::Meta(meta_event) => {
            if let Err(err) = handle_meta_event(
                config,
                redis_connection_manager,
                nats,
                event.transaction_id,
                event.origin,
                meta_event,
            )
            .await
            {
                warn!("Failed to handle meta event: {err:?}");
            }
        }
    }
}

async fn subscribe_to_omni_events(
    consumer: &PullConsumer,
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats: Option<&utils::nats::NatsClient>,
) -> Result<()> {
    let mut messages = consumer
        .messages()
        .await
        .context("Failed to start consuming NATS messages")?;

    while let Some(msg) = messages.next().await {
        let msg = msg.context("NATS message error")?;

        let omni_event: OmniEvent = match serde_json::from_slice(&msg.payload) {
            Ok(event) => event,
            Err(err) => {
                warn!("Failed to deserialize OmniEvent from NATS, terminating message: {err:?}");
                msg.ack_with(async_nats::jetstream::AckKind::Term)
                    .await
                    .ok();
                continue;
            }
        };

        process_nats_message(config, redis_connection_manager, nats, omni_event).await;

        if let Err(err) = msg.ack().await {
            warn!("Failed to ack NATS message: {err:?}");
        }
    }

    Ok(())
}

pub async fn start_indexer_nats(
    config: config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats_client: Arc<utils::nats::NatsClient>,
) -> Result<()> {
    let nats_config = config.nats.as_ref().context("NATS config is not set")?;
    let consumer = nats_client
        .omni_consumer(nats_config)
        .await
        .context("Failed to create NATS consumer")?;

    let nats: Option<&utils::nats::NatsClient> = Some(nats_client.as_ref());

    loop {
        info!("Starting NATS subscription for OmniEvents");

        if let Err(err) =
            subscribe_to_omni_events(&consumer, &config, redis_connection_manager, nats).await
        {
            warn!("Error in NATS subscription: {err:?}");
        }

        warn!("NATS subscription ended, restarting...");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
