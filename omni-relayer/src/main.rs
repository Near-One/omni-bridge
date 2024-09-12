use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use alloy::primitives::Address;
use near_primitives::types::AccountId;

mod defaults;
mod startup;
mod utils;
mod workers;

#[derive(serde::Deserialize, Clone, Debug)]
struct Config {
    redis_url: String,

    bridge_token_factory_address_mainnet: Address,

    token_locker_id_testnet: AccountId,
    bridge_token_factory_address_testnet: Address,
    near_light_client_eth_address_testnet: Address,
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let config: Config = toml::from_str(&std::fs::read_to_string(defaults::CONFIG_FILE)?)?;

    let redis_client = redis::Client::open(config.redis_url.clone())?;

    let jsonrpc_client = near_jsonrpc_client::JsonRpcClient::connect(defaults::NEAR_RPC_TESTNET);
    let near_signer = startup::near::create_signer()?;
    let connector = Arc::new(startup::build_connector(&config, &near_signer)?);

    let (near_sign_transfer_tx, mut near_sign_transfer_rx) = mpsc::unbounded_channel();
    let (eth_finalize_transfer_tx, mut eth_finalize_transfer_rx) = mpsc::unbounded_channel();
    let (eth_finalize_withdraw_tx, mut eth_finalize_withdraw_rx) = mpsc::unbounded_channel();

    tokio::spawn({
        let config = config.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        let near_signer = near_signer.clone();
        async move {
            workers::near::sign_transfer(
                config,
                jsonrpc_client,
                near_signer,
                &mut near_sign_transfer_rx,
            )
            .await;
        }
    });
    tokio::spawn({
        let connector = connector.clone();
        async move {
            workers::near::finalize_transfer(connector, &mut eth_finalize_transfer_rx).await;
        }
    });
    tokio::spawn({
        let connector = connector.clone();
        async move {
            workers::eth::finalize_withdraw(connector, &mut eth_finalize_withdraw_rx).await;
        }
    });

    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async {
            startup::near::start_indexer(
                config,
                redis_client,
                jsonrpc_client,
                near_sign_transfer_tx,
                eth_finalize_transfer_tx,
            )
            .await
            .unwrap();
        }
    });
    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        async {
            startup::eth::start_indexer(config, redis_client, eth_finalize_withdraw_tx)
                .await
                .unwrap();
        }
    });

    tokio::signal::ctrl_c().await?;

    Ok(())
}
