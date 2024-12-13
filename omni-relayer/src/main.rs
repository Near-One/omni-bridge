use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use log::{error, info};
use omni_types::ChainKind;

mod config;
mod startup;
mod utils;
mod workers;

#[derive(Parser)]
struct CliArgs {
    /// Path to the configuration file
    #[clap(short, long, default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    dotenv::dotenv().ok();

    let args = CliArgs::parse();

    let config = toml::from_str::<config::Config>(
        &std::fs::read_to_string(args.config).context("Config file doesn't exist")?,
    )
    .context("Failed to parse config file")?;

    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let jsonrpc_client = near_jsonrpc_client::JsonRpcClient::connect(config.near.rpc_url.clone());
    let near_signer = startup::near::create_signer(config.near.credentials_path.clone())?;

    let connector = Arc::new(startup::build_omni_connector(&config, &near_signer)?);

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
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        async move { workers::near::claim_fee(config, redis_client, connector, jsonrpc_client).await }
    }));

    handles.push(tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        async move {
            workers::evm::finalize_transfer(config, redis_client, connector, jsonrpc_client).await
        }
    }));

    handles.push(tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        async move { startup::near::start_indexer(config, redis_client, jsonrpc_client).await }
    }));
    if config.eth.is_some() {
        handles.push(tokio::spawn({
            let config = config.clone();
            let redis_client = redis_client.clone();
            async move { startup::evm::start_indexer(config, redis_client, ChainKind::Eth).await }
        }));
    }
    if config.base.is_some() {
        handles.push(tokio::spawn({
            let config = config.clone();
            let redis_client = redis_client.clone();
            async move { startup::evm::start_indexer(config, redis_client, ChainKind::Base).await }
        }));
    }
    if config.arb.is_some() {
        handles.push(tokio::spawn({
            let config = config.clone();
            let redis_client = redis_client.clone();
            async move { startup::evm::start_indexer(config, redis_client, ChainKind::Arb).await }
        }));
    }

    if config.solana.is_some() {
        handles.push(tokio::spawn({
            let config = config.clone();
            let redis_client = redis_client.clone();
            async move { startup::solana::start_indexer(config, redis_client).await }
        }));
    }

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
