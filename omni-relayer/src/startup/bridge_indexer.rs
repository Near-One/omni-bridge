use anyhow::Result;
use bridge_indexer_types::documents_types::{
    OmniMetaEvent, OmniTransactionEvent, OmniTransferMessage,
};
use log::{info, warn};
use mongodb::{change_stream::event::ResumeToken, options::ClientOptions, Client, Collection};
use tokio_stream::StreamExt;

use crate::{config, utils};

const OMNI_TRANSACTIONS: &str = "omni_transactions";
const OMNI_META_EVENTS: &str = "omni_meta_events";

async fn watch_omni_transaction_events(
    collection: Collection<OmniTransactionEvent>,
    mut redis_connection: redis::aio::MultiplexedConnection,
) {
    let resume_token: Option<ResumeToken> = utils::redis::get_last_processed::<&str, String>(
        &mut redis_connection,
        utils::redis::OMNI_TRANSACTION_EVENTS_RT,
    )
    .await
    .and_then(|rt| serde_json::from_str(&rt).ok())
    .unwrap_or_default();

    let mut stream = collection.watch().resume_after(resume_token).await.unwrap();

    while let Some(change) = stream.next().await {
        match change {
            Ok(doc) => {
                if let Some(event) = doc.full_document {
                    match event.transfer_message {
                        OmniTransferMessage::NearTransferMessage(transfer_message) => {
                            utils::redis::add_event(
                                &mut redis_connection,
                                utils::redis::EVENTS,
                                transfer_message.origin_nonce.to_string(),
                                crate::workers::Transfer::Near {
                                    transfer_message,
                                    creation_timestamp: chrono::Utc::now().timestamp(),
                                    last_update_timestamp: None,
                                },
                            )
                            .await;
                        }
                        OmniTransferMessage::EvmInitTransferMessage(_init_transfer) => {
                            // TODO: Add evm init transfer handler
                            // We can do it now, since bridge-indexer doesn't have a field for
                            // `sequence`
                            todo!()
                        }
                        OmniTransferMessage::EvmFinTransferMessage(_fin_transfer) => {
                            // TODO: Add evm init transfer handler
                            // We can do it now, since bridge-indexer doesn't have a field for
                            // `sequence`
                            todo!()
                        }
                        OmniTransferMessage::SolanaInitTransfer(init_transfer) => {
                            let Ok(native_fee) = u64::try_from(init_transfer.fee.native_fee.0)
                            else {
                                warn!(
                                    "Failed to parse native fee for Solana transfer: {:?}",
                                    init_transfer
                                );
                                continue;
                            };
                            let Some(emitter) = init_transfer.emitter else {
                                warn!(
                                    "Emitter is not set for Solana transfer: {:?}",
                                    init_transfer
                                );
                                continue;
                            };

                            utils::redis::add_event(
                                &mut redis_connection,
                                utils::redis::EVENTS,
                                event.transaction_id,
                                crate::workers::Transfer::Solana {
                                    amount: init_transfer.amount.0.into(),
                                    token: init_transfer.token.to_string(),
                                    sender: init_transfer.sender.to_string(),
                                    recipient: init_transfer.recipient.to_string(),
                                    fee: init_transfer.fee.fee,
                                    native_fee,
                                    // TODO: Add message field to the bridge-indexer
                                    message: String::new(),
                                    emitter,
                                    // Sequence is the same as origin nonce
                                    sequence: init_transfer.origin_nonce,
                                    creation_timestamp: chrono::Utc::now().timestamp(),
                                    last_update_timestamp: None,
                                },
                            )
                            .await;
                        }
                        OmniTransferMessage::SolanaFinTransfer(fin_transfer) => {
                            let Some(emitter) = fin_transfer.emitter.clone() else {
                                warn!("Emitter is not set for Solana transfer: {:?}", fin_transfer);
                                continue;
                            };
                            let Some(sequence) = fin_transfer.sequence else {
                                warn!(
                                    "Sequence is not set for Solana transfer: {:?}",
                                    fin_transfer
                                );
                                continue;
                            };

                            utils::redis::add_event(
                                &mut redis_connection,
                                utils::redis::EVENTS,
                                event.transaction_id,
                                crate::workers::FinTransfer::Solana { emitter, sequence },
                            )
                            .await;
                        }
                    }
                }
            }
            Err(e) => warn!("Error watching changes: {}", e),
        }

        if let Some(ref resume_token) = stream
            .resume_token()
            .and_then(|rt| serde_json::to_string(&rt).ok())
        {
            utils::redis::update_last_processed(
                &mut redis_connection,
                utils::redis::OMNI_TRANSACTION_EVENTS_RT,
                resume_token,
            )
            .await;
        }
    }
}

pub async fn start_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    info!("Connecting to bridge-indexer");

    let Some(uri) = config.bridge_indexer.mongodb_uri else {
        anyhow::bail!("MONGODB_URI is not set");
    };
    let Some(db_name) = config.bridge_indexer.db_name else {
        anyhow::bail!("DB_NAME is not set");
    };

    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let client_options = ClientOptions::parse(uri).await?;
    let client = Client::with_options(client_options)?;

    let db = client.database(&db_name);
    let omni_transactions_collection = db.collection::<OmniTransactionEvent>(OMNI_TRANSACTIONS);
    let omni_meta_events_collection = db.collection::<OmniMetaEvent>(OMNI_META_EVENTS);

    let handles = vec![
        tokio::spawn(watch_omni_transaction_events(
            omni_transactions_collection,
            redis_connection.clone(),
        )),
        //tokio::spawn(watch_omni_meta_events(
        //    omni_meta_events_collection,
        //    redis_connection.clone(),
        //)),
    ];

    futures::future::join_all(handles).await;

    Ok(())
}
