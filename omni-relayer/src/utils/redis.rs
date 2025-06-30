use omni_types::ChainKind;
use redis::{AsyncCommands, aio::MultiplexedConnection};
use tracing::warn;

use super::bridge_api::TransferFee;

pub const MONGODB_OMNI_EVENTS_RT: &str = "mongodb_omni_events_rt";

pub const EVENTS: &str = "events";
pub const SOLANA_EVENTS: &str = "solana_events";

pub const STUCK_EVENTS: &str = "stuck_events";

pub const FEE_MAPPING: &str = "fee_mapping";

pub const KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR: i64 = 60 * 60 * 24 * 14; // 14 days
pub const CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS: i64 = 60 * 30; // 30 minutes

pub const SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS: u64 = 10;

const QUERY_RETRY_ATTEMPTS: u64 = 10;
const QUERY_RETRY_SLEEP_SECS: u64 = 1;

pub async fn get_fee(
    redis_connection: &mut MultiplexedConnection,
    transfer_id: &str,
) -> Option<TransferFee> {
    for _ in 0..QUERY_RETRY_ATTEMPTS {
        match redis_connection
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
                tokio::time::sleep(tokio::time::Duration::from_secs(QUERY_RETRY_SLEEP_SECS)).await;
            }
        }
    }

    warn!(
        "Failed to get fee for transfer_id {transfer_id} from redis after {QUERY_RETRY_ATTEMPTS} attempts"
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
    redis_connection: &mut MultiplexedConnection,
    key: K,
) -> Option<V>
where
    K: redis::ToRedisArgs + Copy + Send + Sync,
    V: redis::FromRedisValue + Send + Sync,
{
    for _ in 0..QUERY_RETRY_ATTEMPTS {
        if let Ok(res) = redis_connection.get::<K, V>(key).await {
            return Some(res);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(QUERY_RETRY_SLEEP_SECS)).await;
    }

    warn!("Failed to get last processed block from redis db");
    None
}

pub async fn update_last_processed<K, V>(
    redis_connection: &mut MultiplexedConnection,
    key: K,
    value: V,
) where
    K: redis::ToRedisArgs + Copy + Send + Sync,
    V: redis::ToRedisArgs + Copy + Send + Sync,
{
    for _ in 0..QUERY_RETRY_ATTEMPTS {
        if redis_connection.set::<K, V, ()>(key, value).await.is_ok() {
            return;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(QUERY_RETRY_SLEEP_SECS)).await;
    }

    warn!("Failed to update last processed block in redis db");
}

pub async fn get_events(
    redis_connection: &mut MultiplexedConnection,
    key: String,
) -> Option<Vec<(String, String)>> {
    for _ in 0..QUERY_RETRY_ATTEMPTS {
        if let Ok(mut iter) = redis_connection
            .hscan::<String, (String, String)>(key.clone())
            .await
        {
            let mut events = Vec::new();

            while let Some(event) = iter.next_item().await {
                events.push(event);
            }

            return Some(events);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(QUERY_RETRY_SLEEP_SECS)).await;
    }

    warn!("Failed to get events from redis db");
    None
}

pub async fn add_event<F, E>(
    redis_connection: &mut MultiplexedConnection,
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

    for _ in 0..QUERY_RETRY_ATTEMPTS {
        if redis_connection
            .hset::<&str, F, String, ()>(key, field.clone(), serialized_event.clone())
            .await
            .is_ok()
        {
            return;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(QUERY_RETRY_SLEEP_SECS)).await;
    }

    warn!("Failed to add event to redis db");
}

pub async fn remove_event<F>(redis_connection: &mut MultiplexedConnection, key: &str, field: F)
where
    F: redis::ToRedisArgs + Clone + Send + Sync,
{
    for _ in 0..QUERY_RETRY_ATTEMPTS {
        if redis_connection
            .hdel::<&str, F, ()>(key, field.clone())
            .await
            .is_ok()
        {
            return;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(QUERY_RETRY_SLEEP_SECS)).await;
    }

    warn!("Failed to remove event from redis db");
}
