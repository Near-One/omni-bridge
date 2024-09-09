use std::sync::Arc;

use anyhow::Result;

mod defaults;
mod startup;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let near_signer = startup::near::create_signer()?;
    let connector = Arc::new(startup::build_connector(&near_signer)?);

    // NEAR
    tokio::spawn(startup::near::start_indexer(near_signer, connector.clone()));

    // ETH
    tokio::spawn(startup::eth::start_indexer(connector));

    tokio::signal::ctrl_c().await?;

    Ok(())
}
