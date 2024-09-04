use anyhow::Result;
use futures::StreamExt;

use near_jsonrpc_client::{
    methods::{broadcast_tx_commit::RpcBroadcastTxCommitRequest, query::RpcQueryRequest},
    JsonRpcClient,
};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_lake_framework::{
    near_indexer_primitives::{
        views::{ActionView, ReceiptEnumView, ReceiptView},
        IndexerExecutionOutcomeWithReceipt, StreamerMessage,
    },
    LakeConfigBuilder,
};
use near_primitives::{
    transaction::{Transaction, TransactionV0},
    types::{AccountId, BlockReference},
};

const CONTRACT_ID: &str = "omni-locker.test1-dev.testnet";
const SIGN_TRANSFER_GAS: u64 = 300_000_000_000_000;
const SIGN_TRANSFER_ATTACHED_DEPOSIT: u128 = 500_000_000_000_000_000_000_000;

#[derive(Debug, serde::Deserialize)]
struct FtOnTransferLog {
    #[serde(rename = "InitTransferEvent")]
    init_transfer_event: InitTransferEvent,
}

#[derive(Debug, serde::Deserialize)]
struct InitTransferEvent {
    transfer_message: TransferMessage,
}

#[derive(Debug, serde::Deserialize)]
struct TransferMessage {
    origin_nonce: String,
    token: String,
    amount: String,
    recipient: serde_json::Value,
    fee: String,
    sender: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct SignTransferLog {
    #[serde(rename = "SignTransferEvent")]
    sign_transfer_event: SignTransferEvent,
}

#[derive(Debug, serde::Deserialize)]
struct SignTransferEvent {
    signature: SignatureResponse,
    message_payload: TransferMessagePayload,
}

#[derive(Debug, serde::Deserialize)]
pub struct SignatureResponse {
    pub big_r: serde_json::Value,
    pub s: serde_json::Value,
    pub recovery_id: u8,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransferMessagePayload {
    pub nonce: String,
    pub token: AccountId,
    pub amount: String,
    pub recipient: serde_json::Value,
    pub relayer: serde_json::Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = JsonRpcClient::connect("https://rpc.testnet.near.org");

    let config = LakeConfigBuilder::default()
        .testnet()
        .start_block_height(173318000)
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

    let ft_on_transfer_logs = ft_on_transfer_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<FtOnTransferLog>(&log).ok())
        .collect::<Vec<_>>();

    // TODO: This should be wrapped in `tokio::spawn` and error handling
    for log in ft_on_transfer_logs {
        sign_transfer(client, log).await.unwrap();
    }

    let sign_transfer_callback_outcomes = find_sign_transfer_callback_outcomes(&streamer_message);

    let sign_transfer_callback_logs = sign_transfer_callback_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<SignTransferLog>(&log).ok())
        .collect::<Vec<_>>();

    // TODO: call `finalize_deposit_omni_with_logs` using `bridge-sdk`

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

fn find_sign_transfer_callback_outcomes(
    streamer_message: &StreamerMessage,
) -> Vec<IndexerExecutionOutcomeWithReceipt> {
    streamer_message
        .shards
        .iter()
        .flat_map(|shard| shard.receipt_execution_outcomes.iter())
        .filter(|outcome| is_sign_transfer_callback(&outcome.receipt))
        .cloned()
        .collect()
}

fn is_sign_transfer_callback(receipt: &ReceiptView) -> bool {
    receipt.receiver_id == CONTRACT_ID.parse::<AccountId>().unwrap()
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "sign_transfer_callback")
            })
        )
}

async fn sign_transfer(client: &JsonRpcClient, log: FtOnTransferLog) -> Result<()> {
    let signer =
        near_crypto::InMemorySigner::from_secret_key("account_id".parse()?, "private_key".parse()?);

    let access_key_query_response = client
        .call(RpcQueryRequest {
            block_reference: BlockReference::latest(),
            request: near_primitives::views::QueryRequest::ViewAccessKey {
                account_id: signer.account_id.clone(),
                public_key: signer.public_key.clone(),
            },
        })
        .await?;

    let current_nonce = match access_key_query_response.kind {
        QueryResponseKind::AccessKey(access_key) => access_key.nonce,
        _ => anyhow::bail!("Unexpected response"),
    };

    let transaction = TransactionV0 {
        signer_id: signer.account_id.clone(),
        public_key: signer.public_key.clone(),
        nonce: current_nonce + 1,
        receiver_id: CONTRACT_ID.parse()?,
        block_hash: access_key_query_response.block_hash,
        actions: vec![near_primitives::transaction::Action::FunctionCall(
            Box::new(near_primitives::transaction::FunctionCallAction {
                method_name: "sign_transfer".to_string(),
                args: serde_json::json!({ "nonce": log.init_transfer_event.transfer_message.origin_nonce })
                    .to_string()
                    .into_bytes(),
                gas: SIGN_TRANSFER_GAS,
                deposit: SIGN_TRANSFER_ATTACHED_DEPOSIT,
            }),
        )],
    };

    let request = RpcBroadcastTxCommitRequest {
        signed_transaction: Transaction::V0(transaction)
            .sign(&near_crypto::Signer::InMemory(signer)),
    };

    client.call(request).await?;

    Ok(())
}
