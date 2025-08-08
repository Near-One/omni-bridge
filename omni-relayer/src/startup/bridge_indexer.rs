use std::str::FromStr;

use alloy::{primitives::Address, sol_types::SolEvent};
use anyhow::{Context, Result};
use bridge_indexer_types::documents_types::{
    BtcConnectorEvent, BtcConnectorEventDetails, OmniEvent, OmniEventData, OmniMetaEvent,
    OmniMetaEventDetails, OmniTransactionEvent, OmniTransactionOrigin, OmniTransferMessage,
};
use ethereum_types::H256;
use mongodb::{Client, Collection, change_stream::event::ResumeToken, options::ClientOptions};
use omni_types::{ChainKind, Fee, OmniAddress, TransferId, near_events::OmniBridgeEvent};
use solana_sdk::pubkey::Pubkey;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::{
    config, utils,
    workers::{self, RetryableEvent},
};

const OMNI_EVENTS: &str = "omni_events";

fn get_evm_config(config: &config::Config, chain_kind: ChainKind) -> Result<&config::Evm> {
    match chain_kind {
        ChainKind::Eth => config.eth.as_ref().context("EVM config for Eth is not set"),
        ChainKind::Base => config
            .base
            .as_ref()
            .context("EVM config for Base is not set"),
        ChainKind::Arb => config.arb.as_ref().context("EVM config for Arb is not set"),
        _ => anyhow::bail!("Unsupported chain kind for EVM: {:?}", chain_kind),
    }
}

async fn handle_transaction_event(
    config: &config::Config,
    mut redis_connection: redis::aio::MultiplexedConnection,
    origin_transaction_id: String,
    origin: OmniTransactionOrigin,
    event: OmniTransactionEvent,
) -> Result<()> {
    match event.transfer_message {
        OmniTransferMessage::NearTransferMessage(transfer_message) => {
            info!(
                "Received NearTransferMessage: {}",
                transfer_message.origin_nonce
            );

            if transfer_message.recipient.get_chain() != ChainKind::Near {
                utils::redis::add_event(
                    config,
                    &mut redis_connection,
                    utils::redis::EVENTS,
                    transfer_message.origin_nonce.to_string(),
                    RetryableEvent::new(crate::workers::Transfer::Near { transfer_message }),
                )
                .await;
            }
        }
        OmniTransferMessage::NearSignTransferEvent(sign_event) => {
            info!("Received NearSignTransferEvent");

            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                sign_event
                    .message_payload
                    .transfer_id
                    .origin_nonce
                    .to_string(),
                RetryableEvent::new(OmniBridgeEvent::SignTransferEvent {
                    signature: sign_event.signature,
                    message_payload: sign_event.message_payload,
                }),
            )
            .await;
        }
        OmniTransferMessage::NearClaimFeeEvent(_) => {}
        OmniTransferMessage::EvmInitTransferMessage(init_transfer) => {
            info!(
                "Received EvmInitTransferMessage: {}",
                init_transfer.origin_nonce
            );

            let OmniTransactionOrigin::EVMLog {
                block_number,
                block_timestamp,
                chain_kind,
                ..
            } = origin
            else {
                anyhow::bail!("Expected EVMLog for EvmInitTransfer: {:?}", init_transfer);
            };

            let Ok(tx_hash) = H256::from_str(&origin_transaction_id) else {
                anyhow::bail!(
                    "Failed to parse transaction_id as H256: {:?}",
                    origin_transaction_id
                );
            };

            let (OmniAddress::Eth(sender) | OmniAddress::Base(sender) | OmniAddress::Arb(sender)) =
                init_transfer.sender.clone()
            else {
                anyhow::bail!("Unexpected token address: {}", init_transfer.sender);
            };

            let (OmniAddress::Eth(token) | OmniAddress::Base(token) | OmniAddress::Arb(token)) =
                init_transfer.token.clone()
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
                anyhow::bail!(
                    "Failed to parse block_timestamp as i64: {}",
                    block_timestamp
                );
            };

            let expected_finalization_time = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.expected_finalization_time)?;

            let safe_confirmations = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.safe_confirmations)?;

            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id.clone(),
                RetryableEvent::new(workers::Transfer::Evm {
                    chain_kind,
                    block_number,
                    tx_hash,
                    log: log.clone(),
                    creation_timestamp,
                    expected_finalization_time,
                }),
            )
            .await;

            if config.is_fast_relayer_enabled() {
                utils::redis::add_event(
                    config,
                    &mut redis_connection,
                    utils::redis::EVENTS,
                    format!("{origin_transaction_id}_fast"),
                    RetryableEvent::new(crate::workers::Transfer::Fast {
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
                    }),
                )
                .await;
            }
        }
        OmniTransferMessage::EvmFinTransferMessage(fin_transfer) => {
            info!("Received EvmFinTransferMessage");

            let OmniTransactionOrigin::EVMLog {
                block_number,
                block_timestamp,
                chain_kind,
                ..
            } = origin
            else {
                anyhow::bail!("Expected EVMLog for EvmFinTransfer: {:?}", fin_transfer);
            };

            let Ok(tx_hash) = H256::from_str(&origin_transaction_id) else {
                anyhow::bail!(
                    "Failed to parse transaction_id as H256: {:?}",
                    origin_transaction_id
                );
            };

            let Ok(creation_timestamp) = i64::try_from(block_timestamp) else {
                anyhow::bail!(
                    "Failed to parse block_timestamp as i64: {}",
                    block_timestamp
                );
            };

            let expected_finalization_time = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.expected_finalization_time)?;

            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id,
                RetryableEvent::new(workers::FinTransfer::Evm {
                    chain_kind,
                    block_number,
                    tx_hash,
                    topic: utils::evm::FinTransfer::SIGNATURE_HASH,
                    creation_timestamp,
                    expected_finalization_time,
                }),
            )
            .await;
        }
        OmniTransferMessage::SolanaInitTransfer(init_transfer) => {
            info!(
                "Received SolanaInitTransfer: {}",
                init_transfer.origin_nonce
            );

            let OmniAddress::Sol(ref token) = init_transfer.token else {
                anyhow::bail!("Unexpected token address: {}", init_transfer.token);
            };
            let Ok(native_fee) = u64::try_from(init_transfer.fee.native_fee.0) else {
                anyhow::bail!(
                    "Failed to parse native fee for Solana transfer: {:?}",
                    init_transfer
                );
            };
            let Some(emitter) = init_transfer.emitter else {
                anyhow::bail!(
                    "Emitter is not set for Solana transfer: {:?}",
                    init_transfer
                );
            };

            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id,
                RetryableEvent::new(crate::workers::Transfer::Solana {
                    amount: init_transfer.amount.0.into(),
                    token: Pubkey::new_from_array(token.0),
                    sender: init_transfer.sender,
                    recipient: init_transfer.recipient,
                    fee: init_transfer.fee.fee,
                    native_fee,
                    message: init_transfer.message.unwrap_or_default(),
                    emitter: Pubkey::from_str(&emitter).context("Failed to parse emitter")?,
                    sequence: init_transfer.origin_nonce,
                }),
            )
            .await;
        }
        OmniTransferMessage::SolanaFinTransfer(fin_transfer) => {
            info!("Received SolanaFinTransfer");

            let Some(emitter) = fin_transfer.emitter.clone() else {
                anyhow::bail!("Emitter is not set for Solana transfer: {:?}", fin_transfer);
            };
            let Some(sequence) = fin_transfer.sequence else {
                anyhow::bail!(
                    "Sequence is not set for Solana transfer: {:?}",
                    fin_transfer
                );
            };

            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id,
                RetryableEvent::new(crate::workers::FinTransfer::Solana { emitter, sequence }),
            )
            .await;
        }
        OmniTransferMessage::NearFastTransferMessage { .. } => {
            info!("Received NearFastTransferMessage, skipping");
        }
    }

    Ok(())
}

async fn handle_meta_event(
    config: &config::Config,
    mut redis_connection: redis::aio::MultiplexedConnection,
    origin_transaction_id: String,
    origin: OmniTransactionOrigin,
    event: OmniMetaEvent,
) -> Result<()> {
    match event.details {
        OmniMetaEventDetails::EVMDeployToken(deploy_token_event) => {
            info!("Received EVMDeployToken: {deploy_token_event:?}");

            let OmniTransactionOrigin::EVMLog {
                block_number,
                block_timestamp,
                chain_kind,
                ..
            } = origin
            else {
                anyhow::bail!(
                    "Expected EVMLog for EvmDeployToken: {:?}",
                    deploy_token_event
                );
            };

            let Ok(tx_hash) = H256::from_str(&origin_transaction_id) else {
                anyhow::bail!(
                    "Failed to parse transaction_id as H256: {:?}",
                    origin_transaction_id
                );
            };

            let Ok(creation_timestamp) = i64::try_from(block_timestamp) else {
                anyhow::bail!(
                    "Failed to parse block_timestamp as i64: {}",
                    block_timestamp
                );
            };

            let expected_finalization_time = get_evm_config(config, chain_kind)
                .map(|evm_config| evm_config.expected_finalization_time)?;

            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id,
                RetryableEvent::new(workers::DeployToken::Evm {
                    chain_kind,
                    block_number,
                    tx_hash,
                    topic: utils::evm::DeployToken::SIGNATURE_HASH,
                    creation_timestamp,
                    expected_finalization_time,
                }),
            )
            .await;
        }
        OmniMetaEventDetails::SolanaDeployToken {
            emitter, sequence, ..
        } => {
            info!("Received EVMDeployToken: {sequence}");
            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id,
                RetryableEvent::new(workers::DeployToken::Solana { emitter, sequence }),
            )
            .await;
        }
        OmniMetaEventDetails::EVMLogMetadata(_)
        | OmniMetaEventDetails::EVMOnNearEvent { .. }
        | OmniMetaEventDetails::EVMOnNearInternalTransaction { .. }
        | OmniMetaEventDetails::SolanaLogMetadata { .. }
        | OmniMetaEventDetails::NearLogMetadataEvent { .. }
        | OmniMetaEventDetails::NearDeployTokenEvent { .. }
        | OmniMetaEventDetails::NearBindTokenEvent { .. } => {}
    }

    Ok(())
}

async fn handle_btc_event(
    config: &config::Config,
    mut redis_connection: redis::aio::MultiplexedConnection,
    origin_transaction_id: String,
    event: BtcConnectorEvent,
) -> Result<()> {
    match event.details {
        BtcConnectorEventDetails::SignTransaction { relayer, .. } => {
            info!("Received SignBtcTransaction: {origin_transaction_id}");
            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id.clone(),
                RetryableEvent::new(workers::btc::SignBtcTransaction {
                    near_tx_hash: origin_transaction_id,
                    relayer,
                }),
            )
            .await;
        }
        BtcConnectorEventDetails::TransferNearToBtc {
            btc_pending_id,
            utxo_count,
        } => {
            if config.is_signing_btc_transaction_enabled() {
                info!("Received NearToBtcInitTransfer: {origin_transaction_id}");
                for sign_index in 0..utxo_count {
                    info!(
                        "Received sign index {sign_index} for BTC pending ID: {}",
                        btc_pending_id.clone()
                    );
                    utils::redis::add_event(
                        config,
                        &mut redis_connection,
                        utils::redis::EVENTS,
                        origin_transaction_id.clone(),
                        RetryableEvent::new(workers::Transfer::NearToBtc {
                            btc_pending_id: btc_pending_id.clone(),
                            sign_index,
                        }),
                    )
                    .await;
                }
            }
        }
        BtcConnectorEventDetails::TransferBtcToNear {
            btc_tx_hash,
            vout,
            deposit_msg,
        } => {
            info!("Received BtcToNearInitTransfer: {btc_tx_hash}");
            utils::redis::add_event(
                config,
                &mut redis_connection,
                utils::redis::EVENTS,
                origin_transaction_id,
                RetryableEvent::new(workers::Transfer::BtcToNear {
                    btc_tx_hash,
                    vout,
                    deposit_msg,
                }),
            )
            .await;
        }
        BtcConnectorEventDetails::ConfirmedTxHash { btc_tx_hash } => {
            if config.is_verifying_withdraw_enabled() {
                info!("Received ConfirmedTxHash on Btc: {btc_tx_hash}");
                utils::redis::add_event(
                    config,
                    &mut redis_connection,
                    utils::redis::EVENTS,
                    origin_transaction_id,
                    RetryableEvent::new(workers::btc::ConfirmedTxHash { btc_tx_hash }),
                )
                .await;
            }
        }
        BtcConnectorEventDetails::VerifyDeposit { .. }
        | BtcConnectorEventDetails::LogDepositAddress(_) => {}
    }

    Ok(())
}

async fn watch_omni_events_collection(
    collection: &Collection<OmniEvent>,
    config: &config::Config,
    mut redis_connection: redis::aio::MultiplexedConnection,
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
            &mut redis_connection,
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
                                let redis_connection = redis_connection.clone();
                                let config = config.clone();

                                async move {
                                    if let Err(err) = handle_transaction_event(
                                        &config,
                                        redis_connection,
                                        event.transaction_id,
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
                                let redis_connection = redis_connection.clone();
                                let config = config.clone();

                                async move {
                                    if let Err(err) = handle_meta_event(
                                        &config,
                                        redis_connection,
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
                        OmniEventData::BtcConnector(btc_event) => {
                            tokio::spawn({
                                let redis_connection = redis_connection.clone();
                                let config = config.clone();

                                async move {
                                    if let Err(err) = handle_btc_event(
                                        &config,
                                        redis_connection,
                                        event.transaction_id,
                                        btc_event,
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
                &mut redis_connection,
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
    redis_client: redis::Client,
    start_timestamp: Option<u32>,
) -> Result<()> {
    info!("Connecting to bridge-indexer");

    let Some(ref uri) = config.bridge_indexer.mongodb_uri else {
        anyhow::bail!("MONGODB_URI is not set");
    };
    let Some(ref db_name) = config.bridge_indexer.db_name else {
        anyhow::bail!("DB_NAME is not set");
    };

    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let client_options = ClientOptions::parse(uri).await?;
    let client = Client::with_options(client_options)?;

    let db = client.database(db_name);
    let omni_events_collection = db.collection::<OmniEvent>(OMNI_EVENTS);

    loop {
        info!("Starting a mongodb stream that track changes in {OMNI_EVENTS}");

        if let Err(err) = watch_omni_events_collection(
            &omni_events_collection,
            &config,
            redis_connection.clone(),
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
