use omni_types::ChainKind;
use redis::{AsyncCommands, aio::ConnectionManager};
use tracing::warn;

use crate::config;

use super::bridge_api::TransferFee;

pub const MONGODB_OMNI_EVENTS_RT: &str = "mongodb_omni_events_rt";

pub const EVENTS: &str = "events";
pub const SOLANA_EVENTS: &str = "solana_events";

pub const STUCK_EVENTS: &str = "stuck_events";

pub const FEE_MAPPING: &str = "fee_mapping";

pub async fn get_fee(
    config: &config::Config,
    redis_connection_manager: &mut ConnectionManager,
    transfer_id: &str,
) -> Option<TransferFee> {
    for _ in 0..config.redis.query_retry_attempts {
        match redis_connection_manager
            .hget::<&str, &str, Option<String>>(FEE_MAPPING, transfer_id)
            .await
        {
            Ok(Some(serialized)) => match serde_json::from_str(&serialized) {
                Ok(fee) => return Some(fee),
                Err(err) => {
                    warn!("Failed to deserialize Fee for transfer_id {transfer_id}: {err:?}");
                    return None;
                }
            },
            Ok(None) => {
                return None;
            }
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    config.redis.query_retry_sleep_secs,
                ))
                .await;
            }
        }
    }

    warn!(
        "Failed to get fee for transfer_id {transfer_id} from redis after {} attempts",
        config.redis.query_retry_attempts
    );
    None
}

pub fn get_last_processed_key(chain_kind: ChainKind) -> String {
    match chain_kind {
        ChainKind::Sol => "SOLANA_LAST_PROCESSED_SIGNATURE".to_string(),
        _ => format!("{chain_kind:?}_LAST_PROCESSED_BLOCK"),
    }
}

pub async fn get_last_processed<K, V>(
    config: &config::Config,
    redis_connection_manager: &mut ConnectionManager,
    key: K,
) -> Option<V>
where
    K: redis::ToRedisArgs + Copy + Send + Sync,
    V: redis::FromRedisValue + Send + Sync,
{
    for _ in 0..config.redis.query_retry_attempts {
        if let Ok(res) = redis_connection_manager.get::<K, V>(key).await {
            return Some(res);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.redis.query_retry_sleep_secs,
        ))
        .await;
    }

    warn!("Failed to get last processed block from redis db");
    None
}

pub async fn update_last_processed<K, V>(
    config: &config::Config,
    redis_connection: &mut ConnectionManager,
    key: K,
    value: V,
) where
    K: redis::ToRedisArgs + Copy + Send + Sync,
    V: redis::ToRedisArgs + Copy + Send + Sync,
{
    for _ in 0..config.redis.query_retry_attempts {
        if redis_connection.set::<K, V, ()>(key, value).await.is_ok() {
            return;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.redis.query_retry_sleep_secs,
        ))
        .await;
    }

    warn!("Failed to update last processed block in redis db");
}

pub async fn get_events(
    config: &config::Config,
    redis_connection_manager: &mut ConnectionManager,
    key: String,
) -> Option<Vec<(String, String)>> {
    for _ in 0..config.redis.query_retry_attempts {
        if let Ok(mut iter) = redis_connection_manager
            .hscan::<String, (String, String)>(key.clone())
            .await
        {
            let mut events = Vec::new();

            while let Some(event) = iter.next_item().await {
                events.push(event);
            }

            return Some(events);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.redis.query_retry_sleep_secs,
        ))
        .await;
    }

    warn!("Failed to get events from redis db");
    None
}

pub async fn add_event<F, E>(
    config: &config::Config,
    redis_connection_manager: &mut ConnectionManager,
    key: &str,
    field: F,
    event: E,
) where
    F: redis::ToRedisArgs + Clone + Send + Sync,
    E: serde::Serialize + std::fmt::Debug + Send,
{
    let Ok(serialized_event) = serde_json::to_string(&event) else {
        warn!("Failed to serialize event: {event:?}");
        return;
    };

    for _ in 0..config.redis.query_retry_attempts {
        if redis_connection_manager
            .hset::<&str, F, String, ()>(key, field.clone(), serialized_event.clone())
            .await
            .is_ok()
        {
            return;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.redis.query_retry_sleep_secs,
        ))
        .await;
    }

    warn!("Failed to add event to redis db");
}

pub async fn remove_event<F>(
    config: &config::Config,
    redis_connection_manager: &mut ConnectionManager,
    key: &str,
    field: F,
) where
    F: redis::ToRedisArgs + Clone + Send + Sync,
{
    for _ in 0..config.redis.query_retry_attempts {
        if redis_connection_manager
            .hdel::<&str, F, ()>(key, field.clone())
            .await
            .is_ok()
        {
            return;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.redis.query_retry_sleep_secs,
        ))
        .await;
    }

    warn!("Failed to remove event from redis db");
}
