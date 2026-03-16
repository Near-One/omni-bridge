use std::time::Duration;

use anyhow::{Context, Result};
use async_nats::jetstream::{self, consumer};

use crate::config;

pub struct NatsClient {
    jetstream: jetstream::Context,
}

impl NatsClient {
    pub async fn connect(url: &str) -> Result<Self> {
        let username = std::env::var("BRIDGE_NATS_USERNAME")
            .context("BRIDGE_NATS_USERNAME env variable is not set")?;
        let password = std::env::var("BRIDGE_NATS_PASSWORD")
            .context("BRIDGE_NATS_PASSWORD env variable is not set")?;

        let options = async_nats::ConnectOptions::new().user_and_password(username, password);

        let client = options
            .connect(url)
            .await
            .context("Failed to connect to NATS")?;
        let jetstream = jetstream::new(client);
        Ok(Self { jetstream })
    }

    pub async fn omni_consumer(&self, config: &config::Nats) -> Result<consumer::PullConsumer> {
        self.jetstream
            .create_consumer_strict_on_stream(
                consumer::pull::Config {
                    durable_name: Some(config.omni_consumer.name.clone()),
                    ack_policy: consumer::AckPolicy::Explicit,
                    deliver_policy: consumer::DeliverPolicy::Last,
                    max_deliver: config.omni_consumer.max_deliver,
                    filter_subject: config.omni_consumer.subject.clone(),
                    backoff: config
                        .omni_consumer
                        .backoff_secs
                        .iter()
                        .map(|&s| Duration::from_secs(s))
                        .collect(),
                    ..Default::default()
                },
                &config.omni_consumer.stream,
            )
            .await
            .context("Failed to create omni consumer")
    }

    pub async fn publish(&self, subject: String, key: &str, payload: Vec<u8>) -> Result<()> {
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("Nats-Msg-Id", key);

        self.jetstream
            .publish_with_headers(subject, headers, payload.into())
            .await
            .context("Failed to publish work item to NATS")?;

        Ok(())
    }
}
