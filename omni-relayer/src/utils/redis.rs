use log::warn;
use redis::{aio::MultiplexedConnection, AsyncCommands};

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

pub async fn add_event<N, E>(
    redis_connection: &mut MultiplexedConnection,
    key: &str,
    nonce: N,
    event: E,
) where
    N: redis::ToRedisArgs + Send + Sync,
    E: serde::Serialize + Send,
{
    if let Err(err) = redis_connection
        .hset_nx::<&str, N, String, ()>(key, nonce, serde_json::to_string(&event).unwrap())
        .await
    {
        warn!("Failed to add event to redis db: {}", err);
    }
}

pub async fn remove_event<N>(redis_connection: &mut MultiplexedConnection, key: &str, nonce: N)
where
    N: redis::ToRedisArgs + Send + Sync,
{
    if let Err(err) = redis_connection
        .hdel::<String, N, ()>(key.to_string(), nonce)
        .await
    {
        warn!("Failed to remove event from redis db: {}", err);
    }
}
