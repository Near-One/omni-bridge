use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

mod defaults;
mod startup;
mod utils;
mod workers;

#[derive(serde::Deserialize, Clone, Debug)]
struct Config {
    token_locker_id_testnet: String,
    bridge_token_factory_address_testnet: String,
    near_light_client_eth_address_testnet: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let config: Config = toml::from_str(&std::fs::read_to_string(defaults::CONFIG_FILE)?)?;

    let redis_client = redis::Client::open(defaults::REDIS_URL)?;

    let client = near_jsonrpc_client::JsonRpcClient::connect(defaults::NEAR_RPC_TESTNET);
    let near_signer = startup::near::create_signer()?;
    let connector = Arc::new(startup::build_connector(config.clone(), &near_signer)?);

    let (near_sign_transfer_tx, mut near_sign_transfer_rx) = mpsc::unbounded_channel();
    let (eth_finalize_transfer_tx, mut eth_finalize_transfer_rx) = mpsc::unbounded_channel();
    let (eth_finalize_withdraw_tx, mut eth_finalize_withdraw_rx) = mpsc::unbounded_channel();

    tokio::spawn({
        let config = config.clone();
        let client = client.clone();
        let near_signer = near_signer.clone();
        async move {
            workers::near::sign_transfer(config, client, near_signer, &mut near_sign_transfer_rx)
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

    tokio::spawn(startup::near::start_indexer(
        config,
        redis_client,
        client,
        near_sign_transfer_tx,
        eth_finalize_transfer_tx,
    ));
    tokio::spawn(startup::eth::start_indexer(eth_finalize_withdraw_tx));

    tokio::signal::ctrl_c().await?;

    Ok(())
}
