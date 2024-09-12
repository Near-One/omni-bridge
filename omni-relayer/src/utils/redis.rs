use log::warn;
use redis::AsyncCommands;

pub async fn update_last_processed_block(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: &str,
    value: u64,
) {
    if let Err(err) = redis_connection.set::<&str, u64, ()>(key, value).await {
        warn!("Failed to update last processed block in redis db: {}", err);
    }
}

pub async fn add_event<T>(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: &str,
    event: T,
) where
    T: serde::Serialize + Send,
{
    if let Err(err) = redis_connection
        .rpush::<String, String, ()>(key.to_string(), serde_json::to_string(&event).unwrap())
        .await
    {
        warn!("Failed to add event to redis db: {}", err);
    }
}
