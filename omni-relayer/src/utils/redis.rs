use log::warn;
use redis::{aio::MultiplexedConnection, AsyncCommands};

pub const NEAR_LAST_PROCESSED_BLOCK: &str = "near_last_processed_block";
pub const NEAR_INIT_TRANSFER_EVENTS: &str = "near_init_transfer_events";
pub const NEAR_SIGN_TRANSFER_EVENTS: &str = "near_sign_transfer_events";
pub const ETH_LAST_PROCESSED_BLOCK: &str = "eth_last_processed_block";
pub const ETH_WITHDRAW_EVENTS: &str = "eth_withdraw_events";
pub const ETH_DEPOSIT_EVENTS: &str = "eth_withdraw_events";
pub const SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS: u64 = 10;

pub async fn get_last_processed_block(
    redis_connection: &mut MultiplexedConnection,
    key: &str,
) -> Option<u64> {
    match redis_connection.get::<&str, u64>(key).await {
        Ok(res) => Some(res),
        Err(err) => {
            warn!("Failed to get last processed block from redis db: {}", err);
            None
        }
    }
}

pub async fn update_last_processed_block(
    redis_connection: &mut MultiplexedConnection,
    key: &str,
    value: u64,
) {
    if let Err(err) = redis_connection.set::<&str, u64, ()>(key, value).await {
        warn!("Failed to update last processed block in redis db: {}", err);
    }
}

pub async fn get_events(
    redis_connection: &mut MultiplexedConnection,
    key: String,
) -> Option<redis::AsyncIter<(String, String)>> {
    match redis_connection
        .hscan::<String, (String, String)>(key)
        .await
    {
        Ok(res) => Some(res),
        Err(err) => {
            warn!("Failed to get events from redis db: {}", err);
            None
        }
    }
}

pub async fn add_event<F, E>(
    redis_connection: &mut MultiplexedConnection,
    key: &str,
    field: F,
    event: E,
) where
    F: redis::ToRedisArgs + Send + Sync,
    E: serde::Serialize + std::fmt::Debug + Send,
{
    let Ok(serialized_event) = serde_json::to_string(&event) else {
        warn!("Failed to serialize event: {:?}", event);
        return;
    };

    if let Err(err) = redis_connection
        .hset_nx::<&str, F, String, ()>(key, field, serialized_event)
        .await
    {
        warn!("Failed to add event to redis db: {}", err);
    }
}

pub async fn remove_event<F>(redis_connection: &mut MultiplexedConnection, key: &str, field: F)
where
    F: redis::ToRedisArgs + Send + Sync,
{
    if let Err(err) = redis_connection
        .hdel::<String, F, ()>(key.to_string(), field)
        .await
    {
        warn!("Failed to remove event from redis db: {}", err);
    }
}
