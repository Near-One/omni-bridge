use std::sync::Arc;

use anyhow::Result;

use near_jsonrpc_client::JsonRpcClient;

mod defaults;
mod startup;
mod types;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let client = JsonRpcClient::connect(defaults::NEAR_RPC_TESTNET);
    let near_signer = startup::create_near_signer()?;
    let connector = Arc::new(startup::build_connector(&near_signer)?);

    startup::start_near_indexer(client, near_signer, connector).await?;

    Ok(())
}
