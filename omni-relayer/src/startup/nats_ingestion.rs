use std::sync::Arc;

use anyhow::{Context, Result};
use async_nats::jetstream::consumer::PullConsumer;
use bridge_indexer_types::documents_types::{OmniEvent, OmniEventData};
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::{config, utils};

use super::event_handlers::{handle_meta_event, handle_transaction_event};

async fn process_nats_message(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats: Option<&utils::nats::NatsClient>,
    event: OmniEvent,
) -> Result<()> {
    match event.event {
        OmniEventData::Transaction(transaction_event) => {
            handle_transaction_event(
                config,
                redis_connection_manager,
                nats,
                event.transaction_id,
                transaction_event.transfer_id.clone(),
                event.origin,
                transaction_event,
            )
            .await?;
        }
        OmniEventData::Meta(meta_event) => {
            handle_meta_event(
                config,
                redis_connection_manager,
                nats,
                event.transaction_id,
                event.origin,
                meta_event,
            )
            .await?;
        }
    }

    Ok(())
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

        if let Err(err) =
            process_nats_message(config, redis_connection_manager, nats, omni_event).await
        {
            warn!("Failed to process NATS message: {err:?}");
            continue;
        }

        if let Err(err) = msg.ack().await {
            warn!("Failed to ack NATS message: {err:?}");
        }
    }

    Ok(())
}

pub async fn start_indexer_nats(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    nats_client: Arc<utils::nats::NatsClient>,
) -> Result<()> {
    let nats_config = config.nats.as_ref().context("NATS config is not set")?;

    loop {
        info!("Starting NATS subscription for OmniEvents");

        let consumer = match nats_client.omni_consumer(nats_config).await {
            Ok(consumer) => consumer,
            Err(err) => {
                warn!("Failed to create NATS consumer: {err:?}");
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        if let Err(err) = subscribe_to_omni_events(
            &consumer,
            config,
            redis_connection_manager,
            Some(nats_client.as_ref()),
        )
        .await
        {
            warn!("Error in NATS subscription: {err:?}");
        }

        warn!("NATS subscription ended, restarting...");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
