use std::sync::Arc;

use anyhow::{Context, Result};

use near_jsonrpc_client::JsonRpcClient;

use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
};

mod defaults;
mod startup;
mod utils;

sol!(
    #[derive(Debug)]
    event Withdraw(
        string token,
        address indexed sender,
        uint256 amount,
        string recipient,
        address indexed tokenEthAddress
    );
);

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
    let provider = ProviderBuilder::new().on_http(
        defaults::ETH_RPC_MAINNET
            .parse()
            .context("Failed to parse ETH rpc provider as url")?,
    );
    let filter = Filter::new()
        .address(defaults::BRIDGE_TOKEN_FACTORY_ADDRESS_MAINNET.parse::<Address>()?)
        .event("Withdraw(string,address,uint256,string,address)")
        .from_block(20_085_270)
        .to_block(20_085_370);

    let logs = provider.get_logs(&filter).await?;
    for log in logs {
        if let Ok(decoded_log) = log.log_decode::<Withdraw>() {
            log::info!("Decoded log: {:?}", decoded_log);
            let data = decoded_log.data();

            log::info!(
                "Withdraw result: {:?}",
                connector
                    .withdraw(data.token.clone(), data.amount.to(), data.recipient.clone())
                    .await?
            );
        }
    }

    tokio::signal::ctrl_c().await?;

    Ok(())
}
