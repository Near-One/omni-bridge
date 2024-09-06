use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;

use near_crypto::InMemorySigner;
use near_jsonrpc_client::JsonRpcClient;
use near_lake_framework::near_indexer_primitives::StreamerMessage;

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

    let config = startup::create_lake_config(&client).await?;
    let (_, stream) = near_lake_framework::streamer(config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    stream
        .map(|streamer_message| {
            handle_streamer_message(
                &client,
                near_signer.clone(),
                connector.clone(),
                streamer_message,
            )
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
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
