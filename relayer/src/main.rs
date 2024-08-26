use futures::StreamExt;
use near_lake_framework::{
    near_indexer_primitives::{
        views::{ActionView, ReceiptEnumView},
        StreamerMessage,
    },
    LakeConfigBuilder,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = LakeConfigBuilder::default()
        .testnet()
        .start_block_height(82422587)
        .build()
        .expect("Failed to build LakeConfig");

    let (sender, stream) = near_lake_framework::streamer(config);

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(handle_streamer_message)
        .buffer_unordered(1usize);

    while let Some(_handle_message) = handlers.next().await {}

    drop(handlers);

    match sender.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

async fn handle_streamer_message(streamer_message: StreamerMessage) {
    let ft_on_transfer_outcomes = streamer_message
        .shards
        .into_iter()
        .flat_map(|shard| shard.receipt_execution_outcomes)
        .filter(|outcome| {
            matches!(
                outcome.receipt.receipt.clone(),
                ReceiptEnumView::Action { actions, .. } if actions.iter().any(|action| {
                    // TODO: I believe we also need to check receiver_id == lockup account
                    matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "ft_on_transfer")
                })
            )
        })
        .collect::<Vec<_>>();

    if !ft_on_transfer_outcomes.is_empty() {
        println!("{:?}", ft_on_transfer_outcomes);
    }
}
