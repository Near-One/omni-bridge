use std::sync::Arc;

use anyhow::{Context, Result};
use log::{error, info};

use near_crypto::InMemorySigner;
use near_jsonrpc_client::{
    methods::{
        block::RpcBlockRequest, broadcast_tx_commit::RpcBroadcastTxCommitRequest,
        query::RpcQueryRequest,
    },
    JsonRpcClient,
};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_lake_framework::near_indexer_primitives::{
    views::{ActionView, ReceiptEnumView, ReceiptView},
    IndexerExecutionOutcomeWithReceipt, StreamerMessage,
};
use near_primitives::{
    transaction::{Transaction, TransactionV0},
    types::{AccountId, BlockReference},
    views::FinalExecutionOutcomeView,
};

use crate::{defaults, types};

pub async fn get_final_block(client: &JsonRpcClient) -> Result<u64> {
    info!("Getting final block");

    let block_response = RpcBlockRequest {
        block_reference: near_primitives::types::BlockReference::Finality(
            near_primitives::types::Finality::Final,
        ),
    };
    client
        .call(block_response)
        .await
        .map(|block| block.header.height)
        .map_err(Into::into)
}

pub fn process_ft_on_transfer(
    streamer_message: &StreamerMessage,
    client: &JsonRpcClient,
    near_signer: InMemorySigner,
) {
    let ft_on_transfer_outcomes = find_ft_on_transfer_outcomes(streamer_message);

    let ft_on_transfer_logs = ft_on_transfer_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<types::FtOnTransferLog>(&log).ok())
        .collect::<Vec<_>>();

    for log in ft_on_transfer_logs {
        info!("Processing ft_on_transfer_log: {:?}", log);

        let client_clone = client.clone();
        let near_signer_clone = near_signer.clone();

        tokio::spawn(async move {
            if let Err(err) = sign_transfer(client_clone, near_signer_clone, log).await {
                error!("Failed to sign transfer: {}", err);
            }
        });
    }
}

pub fn process_sign_transfer_callback(
    streamer_message: StreamerMessage,
    connector: Arc<nep141_connector::Nep141Connector>,
) {
    let sign_transfer_callback_outcomes = find_sign_transfer_callback_outcomes(&streamer_message);
    let sign_transfer_callback_logs = sign_transfer_callback_outcomes
        .iter()
        .flat_map(|outcome| outcome.execution_outcome.outcome.logs.clone())
        .filter_map(|log| serde_json::from_str::<types::SignTransferLog>(&log).ok())
        .collect::<Vec<_>>();

    for log in sign_transfer_callback_logs {
        info!("Processing sign_transfer_callback_log: {:?}", log);

        let connector_clone = connector.clone();
        let Ok(serialized_log) = serde_json::to_string(&log.sign_transfer_event) else {
            error!("Failed to serialize log: {:?}", log);
            continue;
        };

        tokio::spawn(async move {
            if let Err(err) = connector_clone
                .finalize_deposit_omni_with_log(&serialized_log)
                .await
            {
                error!("Failed to finalize deposit: {}", err);
            }
        });
    }
}

fn find_ft_on_transfer_outcomes(
    streamer_message: &StreamerMessage,
) -> Vec<IndexerExecutionOutcomeWithReceipt> {
    streamer_message
        .shards
        .iter()
        .flat_map(|shard| shard.receipt_execution_outcomes.iter())
        .filter(|outcome| {
            if let Ok(res) = is_ft_on_transfer(&outcome.receipt) {
                res
            } else {
                false
            }
        })
        .cloned()
        .collect()
}

fn is_ft_on_transfer(receipt: &ReceiptView) -> Result<bool> {
    Ok(receipt.receiver_id
        == defaults::TOKEN_LOCKER_ID_TESTNET
            .parse::<AccountId>()
            .context("Failed to parse AccountId")?
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "ft_on_transfer")
            })
        ))
}

fn find_sign_transfer_callback_outcomes(
    streamer_message: &StreamerMessage,
) -> Vec<IndexerExecutionOutcomeWithReceipt> {
    streamer_message
        .shards
        .iter()
        .flat_map(|shard| shard.receipt_execution_outcomes.iter())
        .filter(|outcome| {
            if let Ok(res) = is_sign_transfer_callback(&outcome.receipt) {
                res
            } else {
                false
            }
        })
        .cloned()
        .collect()
}

fn is_sign_transfer_callback(receipt: &ReceiptView) -> Result<bool> {
    Ok(receipt.receiver_id
        == defaults::TOKEN_LOCKER_ID_TESTNET
            .parse::<AccountId>()
            .context("Failed to parse AccountId")?
        && matches!(
            receipt.receipt,
            ReceiptEnumView::Action { ref actions, .. } if actions.iter().any(|action| {
                matches!(action, ActionView::FunctionCall { method_name, .. } if method_name == "sign_transfer_callback")
            })
        ))
}

async fn sign_transfer(
    client: JsonRpcClient,
    near_signer: InMemorySigner,
    log: types::FtOnTransferLog,
) -> Result<FinalExecutionOutcomeView> {
    let access_key_query_response = client
        .call(RpcQueryRequest {
            block_reference: BlockReference::latest(),
            request: near_primitives::views::QueryRequest::ViewAccessKey {
                account_id: near_signer.account_id.clone(),
                public_key: near_signer.public_key.clone(),
            },
        })
        .await?;

    let current_nonce = match access_key_query_response.kind {
        QueryResponseKind::AccessKey(access_key) => access_key.nonce,
        _ => anyhow::bail!("Failed to get current nonce"),
    };

    let transaction = TransactionV0 {
        signer_id: near_signer.account_id.clone(),
        public_key: near_signer.public_key.clone(),
        nonce: current_nonce + 1,
        receiver_id: defaults::TOKEN_LOCKER_ID_TESTNET.parse()?,
        block_hash: access_key_query_response.block_hash,
        actions: vec![near_primitives::transaction::Action::FunctionCall(
            Box::new(near_primitives::transaction::FunctionCallAction {
                method_name: "sign_transfer".to_string(),
                args: serde_json::json!({ "nonce": log.init_transfer_event.transfer_message.origin_nonce })
                    .to_string()
                    .into_bytes(),
                gas: defaults::SIGN_TRANSFER_GAS,
                deposit: defaults::SIGN_TRANSFER_ATTACHED_DEPOSIT,
            }),
        )],
    };

    let request = RpcBroadcastTxCommitRequest {
        signed_transaction: Transaction::V0(transaction)
            .sign(&near_crypto::Signer::InMemory(near_signer)),
    };

    client
        .call(request)
        .await
        .map_err(|err| anyhow::anyhow!(err))
}
