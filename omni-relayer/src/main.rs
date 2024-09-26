use std::sync::Arc;

use anyhow::{Context, Result};
use log::{error, info};

mod config;
mod startup;
mod utils;
mod workers;

const CONFIG_FILE: &str = "config.toml";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let mut config = toml::from_str::<config::Config>(&std::fs::read_to_string(CONFIG_FILE)?)?;

    config.eth.rpc_ws_url = config.eth.rpc_ws_url.replace(
        "API-KEY",
        &std::env::var("EVM_RPC_WS_API_KEY")
            .context("Failed to get `EVM_RPC_WS_API_KEY` env variable")?,
    );

    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let jsonrpc_client = near_jsonrpc_client::JsonRpcClient::connect(config.near.rpc_url.clone());
    let near_signer = startup::near::create_signer(config.near.credentials_path.clone())?;

    let connector = Arc::new(startup::build_connector(&config, &near_signer)?);

    let mut handles = Vec::new();

    handles.push(tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move { workers::near::sign_transfer(config, redis_client, connector).await }
    }));
    handles.push(tokio::spawn({
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move { workers::near::finalize_transfer(redis_client, connector).await }
    }));
    handles.push(tokio::spawn({
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move { workers::near::claim_fee(redis_client, connector).await }
    }));
    handles.push(tokio::spawn({
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move { workers::eth::finalize_withdraw(redis_client, connector).await }
    }));

    handles.push(tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async move { startup::near::start_indexer(config, redis_client, jsonrpc_client).await }
    }));
    handles.push(tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async move { startup::eth::start_indexer(config, redis_client).await }
    }));

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C signal, shutting down.");
        }
        result = futures::future::select_all(handles) => {
            let (res, _, _) = result;
            if let Ok(Err(err)) = res {
                error!("A worker encountered an error: {:?}", err);
            }
        }
    }

    Ok(())
}
