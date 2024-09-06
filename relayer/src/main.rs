use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;

use log::info;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::{near_indexer_primitives::StreamerMessage, LakeConfigBuilder};

mod defaults;
mod types;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let client = JsonRpcClient::connect(defaults::NEAR_RPC_TESTNET);

    let near_signer = near_crypto::InMemorySigner::from_secret_key(
        std::env::var("NEAR_ACCOUNT_ID")
            .context("Failed to get `NEAR_ACCOUNT_ID` env variable")?
            .parse()?,
        std::env::var("NEAR_PRIVATE_KEY")
            .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?
            .parse()?,
    );

    let connector = Arc::new(
        nep141_connector::Nep141ConnectorBuilder::default()
            .eth_endpoint(Some(defaults::ETH_RPC_TESTNET.to_string()))
            .eth_chain_id(Some(defaults::ETH_CHAIN_ID_TESTNET))
            .near_endpoint(Some(defaults::NEAR_RPC_TESTNET.to_string()))
            .token_locker_id(Some(defaults::TOKEN_LOCKER_ID_TESTNET.to_string()))
            .bridge_token_factory_address(Some(
                defaults::BRIDGE_TOKEN_FACTORY_ADDRESS_TESTNET.to_string(),
            ))
            .near_light_client_address(Some(
                defaults::NEAR_LIGHT_CLIENT_ETH_ADDRESS_TESTNET.to_string(),
            ))
            .eth_private_key(Some(
                std::env::var("ETH_PRIVATE_KEY")
                    .context("Failed to get `NEAR_PRIVATE_KEY` env variable")?,
            ))
            .near_signer(Some(near_signer.account_id.to_string()))
            .near_private_key(Some(near_signer.secret_key.to_string()))
            .build()
            .context("Failed to build Nep141Connector")?,
    );

    let final_block = utils::get_final_block(&client).await?;
    info!("Starting NEAR Lake from block: {}", final_block);

    let config = LakeConfigBuilder::default()
        .testnet()
        // TODO: Add `start_from_interrupted` option
        .start_block_height(final_block)
        .build()
        .context("Failed to build LakeConfig")?;

    let (sender, stream) = near_lake_framework::streamer(config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    let mut handlers = stream
        .map(|streamer_message| {
            handle_streamer_message(
                &client,
                near_signer.clone(),
                connector.clone(),
                streamer_message,
            )
        })
        .buffer_unordered(10);

    while handlers.next().await.is_some() {}

    match sender.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(e.into()),
    }
}

async fn handle_streamer_message(
    client: &JsonRpcClient,
    near_signer: InMemorySigner,
    connector: Arc<nep141_connector::Nep141Connector>,
    streamer_message: StreamerMessage,
) {
    utils::process_ft_on_transfer(&streamer_message, client, near_signer);
    utils::process_sign_transfer_callback(streamer_message, connector);
}
