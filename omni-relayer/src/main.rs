use std::sync::Arc;

use anyhow::Result;

use near_jsonrpc_client::JsonRpcClient;

mod defaults;
mod startup;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    // NEAR
    let client = JsonRpcClient::connect(defaults::NEAR_RPC_TESTNET);
    let near_signer = startup::near::create_signer()?;
    let connector = Arc::new(startup::build_connector(&near_signer)?);

    tokio::spawn(startup::near::start_indexer(
        client,
        near_signer,
        connector.clone(),
    ));

    // ETH
    tokio::spawn(startup::eth::start_indexer(connector));

    tokio::signal::ctrl_c().await?;

    Ok(())
}
