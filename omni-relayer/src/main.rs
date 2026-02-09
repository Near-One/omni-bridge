use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use config::Network;
use near_sdk::base64::{Engine, engine::general_purpose};
use omni_types::ChainKind;
use reqwest::Url;
use solana_sdk::signature::Signature;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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
    /// Start block for Bnb indexer
    #[clap(long)]
    bnb_start_block: Option<u64>,
    /// Start block for Pol indexer
    #[clap(long)]
    pol_start_block: Option<u64>,
    /// Start signature for Solana indexer
    #[clap(long)]
    solana_start_signature: Option<Signature>,
    /// Start timestamp for bridge indexer
    #[clap(long)]
    start_timestamp: Option<u32>,
}

fn init_logging(network: Network) -> Result<()> {
    let fmt_layer = fmt::Layer::default()
        .with_timer(fmt::time::ChronoLocal::rfc_3339())
        .with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let grafana_loki_url = std::env::var("GRAFANA_LOKI_URL").ok();
    let grafana_loki_user = std::env::var("GRAFANA_LOKI_USER").ok();
    let grafana_api_key = std::env::var("GRAFANA_CLOUD_API_KEY").ok();

    if let (Some(url), Some(user), Some(key)) =
        (grafana_loki_url, grafana_loki_user, grafana_api_key)
    {
        let basic = format!("{user}:{key}");
        let encoded = general_purpose::STANDARD.encode(basic);

        let base = Url::parse(&url).context("Failed to parse `GRAFANA_LOKI_URL` as a valid URL")?;

        let (loki_layer, loki_task) = tracing_loki::builder()
            .label("app", format!("omni-relayer-{network}"))?
            .http_header("Authorization", format!("Basic {encoded}"))?
            .build_url(base)?;

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(loki_layer)
            .try_init()
            .context("failed to initialize tracing subscriber with Loki")?;

        tokio::spawn(loki_task);

        info!("Loki logging enabled");
    } else {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .try_init()
            .context("failed to initialize basic tracing subscriber")?;

        warn!(
            "Running without Loki due to missing one of `GRAFANA_LOKI_URL`, `GRAFANA_LOKI_USER` or `GRAFANA_CLOUD_API_KEY` environment variables"
        );
    }

    Ok(())
}

async fn build_redis_connection_manager(
    config: &config::Config,
) -> Result<redis::aio::ConnectionManager> {
    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let redis_connection_manager = redis::aio::ConnectionManager::new_with_config(
        redis_client,
        redis::aio::ConnectionManagerConfig::new().set_response_timeout(
            tokio::time::Duration::from_secs(config.redis.query_timeout_secs),
        ),
    )
    .await?;
    Ok(redis_connection_manager)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let args = CliArgs::parse();

    let config = toml::from_str::<config::Config>(
        &std::fs::read_to_string(args.config).context("Config file doesn't exist")?,
    )
    .context("Failed to parse config file")?;

    init_logging(config.near.network).context("Failed to initialize logging")?;

    let redis_connection_manager = build_redis_connection_manager(&config)
        .await
        .context("Failed to create Redis connection manager")?;
    let jsonrpc_client = near_jsonrpc_client::JsonRpcClient::connect(config.near.rpc_url.clone());

    let near_omni_signer = startup::near::get_signer(&config, config::NearSignerType::Omni)?;
    let omni_connector = Arc::new(startup::build_omni_connector(&config, &near_omni_signer)?);

    let (near_fast_signer, fast_connector) = if config.is_fast_relayer_enabled() {
        let near_fast_signer = startup::near::get_signer(&config, config::NearSignerType::Fast)?;

        (
            Some(near_fast_signer.clone()),
            Arc::new(startup::build_omni_connector(&config, &near_fast_signer)?),
        )
    } else {
        (None, Arc::default())
    };

    let near_omni_nonce = Arc::new(utils::nonce::NonceManager::new(
        utils::nonce::ChainClient::Near {
            jsonrpc_client: jsonrpc_client.clone(),
            signer: Box::new(near_omni_signer),
        },
    ));
    let near_fast_nonce = near_fast_signer.map(|near_fast_signer| {
        Arc::new(utils::nonce::NonceManager::new(
            utils::nonce::ChainClient::Near {
                jsonrpc_client: jsonrpc_client.clone(),
                signer: Box::new(near_fast_signer),
            },
        ))
    });
    let evm_nonces = Arc::new(utils::nonce::EvmNonceManagers::new(&config));

    let mut handles = Vec::new();

    if config.is_bridge_indexer_enabled() {
        handles.push(tokio::spawn({
            let config = config.clone();
            let mut redis_connection_manager = redis_connection_manager.clone();
            async move {
                startup::bridge_indexer::start_indexer(
                    config,
                    &mut redis_connection_manager,
                    args.start_timestamp,
                )
                .await
            }
        }));
    } else {
        handles.push(tokio::spawn({
            let config = config.clone();
            let mut redis_connection_manager = redis_connection_manager.clone();
            let jsonrpc_client = jsonrpc_client.clone();
            async move {
                startup::near::start_indexer(
                    config,
                    &mut redis_connection_manager,
                    jsonrpc_client,
                    args.near_start_block,
                )
                .await
            }
        }));
        if config.eth.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let mut redis_connection_manager = redis_connection_manager.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        &mut redis_connection_manager,
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
                let mut redis_connection_manager = redis_connection_manager.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        &mut redis_connection_manager,
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
                let mut redis_connection_manager = redis_connection_manager.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        &mut redis_connection_manager,
                        ChainKind::Arb,
                        args.arb_start_block,
                    )
                    .await
                }
            }));
        }
        if config.bnb.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let mut redis_connection_manager = redis_connection_manager.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        &mut redis_connection_manager,
                        ChainKind::Bnb,
                        args.bnb_start_block,
                    )
                    .await
                }
            }));
        }
        if config.pol.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let mut redis_connection_manager = redis_connection_manager.clone();
                async move {
                    startup::evm::start_indexer(
                        config,
                        &mut redis_connection_manager,
                        ChainKind::Pol,
                        args.pol_start_block,
                    )
                    .await
                }
            }));
        }
        if config.solana.is_some() {
            handles.push(tokio::spawn({
                let config = config.clone();
                let mut redis_connection_manager = redis_connection_manager.clone();
                async move {
                    startup::solana::start_indexer(
                        &config,
                        &mut redis_connection_manager,
                        args.solana_start_signature,
                    )
                    .await
                }
            }));
            handles.push(tokio::spawn({
                let config = config.clone();
                let mut redis_connection_manager = redis_connection_manager.clone();
                async move {
                    startup::solana::process_signature(&config, &mut redis_connection_manager).await
                }
            }));
        }
    }

    handles.push(tokio::spawn({
        let config = config.clone();
        let redis_connection_manager = redis_connection_manager.clone();
        let omni_connector = omni_connector.clone();
        let fast_connector = fast_connector.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        let near_omni_nonce = near_omni_nonce.clone();
        let near_fast_nonce = near_fast_nonce.clone();
        let evm_nonces = evm_nonces.clone();

        async move {
            workers::process_events(
                config,
                redis_connection_manager,
                omni_connector,
                fast_connector,
                jsonrpc_client,
                near_omni_nonce,
                near_fast_nonce,
                evm_nonces,
            )
            .await
        }
    }));

    if config.is_fee_bumping_enabled(ChainKind::Eth) {
        handles.push(tokio::spawn({
            let config = config.clone();
            let mut redis_connection_manager = redis_connection_manager.clone();
            async move {
                startup::evm_fee_bumping::start_evm_fee_bumping(
                    config,
                    ChainKind::Eth,
                    &mut redis_connection_manager,
                )
                .await
            }
        }));
    }

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
