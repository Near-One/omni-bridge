use anyhow::Result;
use bridge_indexer_types::documents_types::{OmniEvent, OmniEventData};
use mongodb::{Client, Collection, change_stream::event::ResumeToken, options::ClientOptions};
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::{config, utils};

use super::event_handlers::{handle_meta_event, handle_transaction_event};

const OMNI_EVENTS: &str = "omni_events";

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
                            if config.bridge_indexer.is_whitelist_active() {
                                continue;
                            }

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
    config: &config::Config,
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
            config,
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
