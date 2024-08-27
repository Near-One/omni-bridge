use anyhow::Result;
use futures::StreamExt;
use near_jsonrpc_client::{methods::query::RpcQueryRequest, JsonRpcClient};
use near_lake_framework::{
    near_indexer_primitives::{
        views::{ActionView, ReceiptEnumView, ReceiptView},
        IndexerExecutionOutcomeWithReceipt, StreamerMessage,
    },
    LakeConfigBuilder,
};
use near_primitives::{
    types::{AccountId, BlockReference, FunctionArgs},
    views::QueryRequest,
};

const CONTRACT_ID: &str = "omni-locker.test1-dev.testnet";

#[derive(Debug, serde::Deserialize)]
struct FungibleTokenOnTransfer {
    origin_nonce: String,
    token: String,
    amount: String,
    recipient: serde_json::Value,
    fee: String,
    sender: serde_json::Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = JsonRpcClient::connect("https://rpc.testnet.near.org");

    let config = LakeConfigBuilder::default()
        .testnet()
        .start_block_height(172306861)
        .build()
        .expect("Failed to build LakeConfig");

    let (sender, stream) = near_lake_framework::streamer(config);
    let stream = tokio_stream::wrappers::ReceiverStream::new(stream);

    let mut handlers = stream
        .map(|streamer_message| handle_streamer_message(&client, streamer_message))
        .buffer_unordered(1);

    while handlers.next().await.is_some() {}

    match sender.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(e.into()),
    }
}

async fn handle_streamer_message(
    client: &JsonRpcClient,
    streamer_message: StreamerMessage,
) -> Result<()> {
    let ft_on_transfer_outcomes = find_ft_on_transfer_outcomes(&streamer_message);

    let logs = ft_on_transfer_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<FungibleTokenOnTransfer>(&log).ok())
        .collect::<Vec<_>>();

    println!("Logs: {:?}", logs);

    // TODO: This should be wrapped in `tokio::spawn` and error handling
    for log in logs {
        sign_transfer(client, log).await.unwrap();
    }

    Ok(())
}

fn find_ft_on_transfer_outcomes(
    streamer_message: &StreamerMessage,
) -> Vec<IndexerExecutionOutcomeWithReceipt> {
    streamer_message
        .shards
        .iter()
        .flat_map(|shard| shard.receipt_execution_outcomes.iter())
        .filter(|outcome| is_ft_on_transfer(&outcome.receipt))
        .cloned()
        .collect()
}

fn is_ft_on_transfer(receipt: &ReceiptView) -> bool {
    receipt.receiver_id == CONTRACT_ID.parse::<AccountId>().unwrap()
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "ft_on_transfer")
            })
        )
}

async fn sign_transfer(client: &JsonRpcClient, log: FungibleTokenOnTransfer) -> Result<()> {
    let request = RpcQueryRequest {
        block_reference: BlockReference::latest(),
        request: QueryRequest::CallFunction {
            account_id: CONTRACT_ID.parse()?,
            method_name: "sign_transfer".to_string(),
            args: FunctionArgs::from(
                serde_json::json!({ "nonce": log.origin_nonce })
                    .to_string()
                    .into_bytes(),
            ),
        },
    };

    let server_status = client.call(request).await?;

    println!("Response: {:?}", server_status);

    Ok(())
}
