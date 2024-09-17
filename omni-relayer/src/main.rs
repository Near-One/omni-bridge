use std::sync::Arc;

use anyhow::Result;

mod config;
mod startup;
mod utils;
mod workers;

const CONFIG_FILE: &str = "config.toml";

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    println!("{}", u128::MAX);
    println!("{}", 500_000_000_000_000_000_000_000u128);

    let config = toml::from_str::<config::Config>(&std::fs::read_to_string(CONFIG_FILE)?)?;

    let redis_client = redis::Client::open(config.redis.url.clone())?;

    let jsonrpc_client =
        near_jsonrpc_client::JsonRpcClient::connect(config.testnet.near_rpc_url.clone());
    let near_signer = startup::near::create_signer()?;
    let connector = Arc::new(startup::build_connector(&config, &near_signer)?);

    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            workers::near::sign_transfer(config, redis_client, connector).await;
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            workers::near::finalize_transfer(config, redis_client, connector).await;
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async move {
            workers::near::claim_fee(config, redis_client).await;
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            workers::eth::finalize_withdraw(config, redis_client, connector).await;
        }
    });

    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async {
            startup::near::start_indexer(config, redis_client, jsonrpc_client)
                .await
                .unwrap();
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async {
            startup::eth::start_indexer(config, redis_client)
                .await
                .unwrap();
        }
    });

    tokio::signal::ctrl_c().await?;

    Ok(())
}
