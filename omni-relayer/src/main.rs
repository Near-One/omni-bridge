use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use log::{error, info};
use omni_types::ChainKind;
use solana_sdk::signature::Signature;

mod config;
mod startup;
mod utils;
mod workers;

#[derive(Parser)]
struct CliArgs {
    /// Path to the configuration file
    #[clap(short, long, default_value = "config.toml")]
    config: String,
    /// Start block for Near indexer
    #[clap(long)]
    near_start_block: Option<u64>,
    /// Start block for Ethereum indexer
    #[clap(long)]
    eth_start_block: Option<u64>,
    /// Start block for Base indexer
    #[clap(long)]
    base_start_block: Option<u64>,
    /// Start block for Arbitrum indexer
    #[clap(long)]
    arb_start_block: Option<u64>,
    /// Start signature for Solana indexer
    #[clap(long)]
    solana_start_signature: Option<Signature>,
    /// Start timestamp for bridge indexer
    #[clap(long)]
    start_timestamp: Option<i64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init_timed();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let args = CliArgs::parse();

    let config = toml::from_str::<config::Config>(
        &std::fs::read_to_string(args.config).context("Config file doesn't exist")?,
    )
    .context("Failed to parse config file")?;

    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let jsonrpc_client = near_jsonrpc_client::JsonRpcClient::connect(config.near.rpc_url.clone());
    let near_signer = startup::near::get_signer(config.near.credentials_path.as_ref())?;

    let connector = Arc::new(startup::build_omni_connector(&config, &near_signer)?);

    let near_nonce = Arc::new(utils::nonce::NonceManager::new(
        utils::nonce::ChainClient::Near {
            jsonrpc_client: jsonrpc_client.clone(),
            signer: near_signer,
        },
    ));
    let evm_nonces = Arc::new(utils::nonce::EvmNonceManagers::new(&config));

    let mut handles = Vec::new();

    if config.is_bridge_indexer_enabled() {
        handles.push(tokio::spawn({
            let config = config.clone();
            let redis_client = redis_client.clone();
            async move {
                startup::bridge_indexer::start_indexer(config, redis_client, args.start_timestamp)
                    .await
            }
        }));
    } else {
        handles.push(tokio::spawn({
            let config = config.clone();
            let redis_client = redis_client.clone();
            let jsonrpc_client = jsonrpc_client.clone();
            async move {
                startup::near::start_indexer(
                    config,
                    redis_client,
                    jsonrpc_client,
                    args.near_start_block,
                )
                .await
            }
        }));
        if config.eth.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let redis_client = redis_client.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        redis_client,
                        ChainKind::Eth,
                        args.eth_start_block,
                    )
                    .await
                }
            }));
        }
        if config.base.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let redis_client = redis_client.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        redis_client,
                        ChainKind::Base,
                        args.base_start_block,
                    )
                    .await
                }
            }));
        }
        if config.arb.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let redis_client = redis_client.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        redis_client,
                        ChainKind::Arb,
                        args.arb_start_block,
                    )
                    .await
                }
            }));
        }
        if config.solana.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let redis_client = redis_client.clone();
                async move {
                    startup::solana::start_indexer(
                        config,
                        redis_client,
                        args.solana_start_signature,
                    )
                    .await
                }
            }));
            handles.push(tokio::spawn({
                let config = config.clone();
                let redis_client = redis_client.clone();
                async move { startup::solana::process_signature(config, redis_client).await }
            }));
        }
    }

    handles.push(tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        let near_nonce = near_nonce.clone();
        let evm_nonces = evm_nonces.clone();

        async move {
            workers::process_events(
                config,
                redis_client,
                connector,
                jsonrpc_client,
                near_nonce,
                evm_nonces,
            )
            .await
        }
    }));

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C signal, shutting down.");
        }
        result = futures::future::select_all(handles) => {
            let (res, _, _) = result;
            if let Ok(Err(err)) = res {
                error!("A worker encountered an error: {err:?}");
            }
        }
    }

    Ok(())
}
