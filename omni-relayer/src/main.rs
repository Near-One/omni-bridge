use std::sync::Arc;

use anyhow::Result;
use log::error;

mod config;
mod startup;
mod utils;
mod workers;

const CONFIG_FILE: &str = "config.toml";

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

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
            if let Err(err) = workers::near::sign_transfer(config, redis_client, connector).await {
                error!("Error in sign_transfer: {:?}", err);
            }
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            if let Err(err) =
                workers::near::finalize_transfer(config, redis_client, connector).await
            {
                error!("Error in finalize_transfer: {:?}", err);
            }
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            if let Err(err) = workers::near::claim_fee(config, redis_client, connector).await {
                error!("Error in claim_fee: {:?}", err);
            }
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            if let Err(err) = workers::eth::finalize_withdraw(config, redis_client, connector).await
            {
                error!("Error in finalize_withdraw: {:?}", err);
            }
        }
    });

    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async move {
            if let Err(err) =
                startup::near::start_indexer(config, redis_client, jsonrpc_client).await
            {
                error!("Error in near start_indexer: {:?}", err);
            }
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async move {
            if let Err(err) = startup::eth::start_indexer(config, redis_client).await {
                error!("Error in eth start_indexer: {:?}", err);
            }
        }
    });

    tokio::signal::ctrl_c().await?;

    Ok(())
}
