use std::sync::Arc;

use anyhow::Result;

use alloy::primitives::Address;
use near_primitives::types::AccountId;

mod defaults;
mod startup;
mod utils;
mod workers;

#[derive(serde::Deserialize, Clone, Debug)]
struct Config {
    bridge_token_factory_address_mainnet: Address,

    token_locker_id_testnet: AccountId,
    bridge_token_factory_address_testnet: Address,
    near_light_client_eth_address_testnet: Address,
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let config: Config = toml::from_str(&std::fs::read_to_string(defaults::CONFIG_FILE)?)?;

    let redis_client = redis::Client::open(defaults::REDIS_URL)?;

    let jsonrpc_client = near_jsonrpc_client::JsonRpcClient::connect(defaults::NEAR_RPC_TESTNET);
    let near_signer = startup::near::create_signer()?;
    let connector = Arc::new(startup::build_connector(&config, &near_signer)?);

    tokio::spawn({
        let config = config.clone();
        let redis_client = redis_client.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        let near_signer = near_signer.clone();
        async move {
            workers::near::sign_transfer(config, redis_client, jsonrpc_client, near_signer).await;
        }
    });
    tokio::spawn({
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            workers::near::finalize_transfer(redis_client, connector).await;
        }
    });
    tokio::spawn({
        let redis_client = redis_client.clone();
        let connector = connector.clone();
        async move {
            workers::eth::finalize_withdraw(redis_client, connector).await;
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
