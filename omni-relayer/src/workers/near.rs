use std::sync::Arc;

use log::{error, info, warn};
use nep141_connector::Nep141Connector;
use tokio::sync::mpsc;

use near_crypto::InMemorySigner;
use near_jsonrpc_client::{
    methods::{broadcast_tx_commit::RpcBroadcastTxCommitRequest, query::RpcQueryRequest},
    JsonRpcClient,
};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::{
    transaction::{Transaction, TransactionV0},
    types::BlockReference,
};
use omni_types::near_events::Nep141LockerEvent;

use crate::defaults;

pub async fn sign_transfer(
    config: crate::Config,
    client: JsonRpcClient,
    near_signer: InMemorySigner,
    sign_transfer_rx: &mut mpsc::UnboundedReceiver<Nep141LockerEvent>,
) {
    while let Some(log) = sign_transfer_rx.recv().await {
        let Nep141LockerEvent::InitTransferEvent { transfer_message } = log else {
            warn!("Expected InitTransferEvent, got: {:?}", log);
            continue;
        };

        let near_signer = near_signer.clone();

        let Ok(access_key_query_response) = client
            .call(RpcQueryRequest {
                block_reference: BlockReference::latest(),
                request: near_primitives::views::QueryRequest::ViewAccessKey {
                    account_id: near_signer.account_id.clone(),
                    public_key: near_signer.public_key.clone(),
                },
            })
            .await
        else {
            warn!("Failed to get access key");
            continue;
        };

        let current_nonce =
            if let QueryResponseKind::AccessKey(access_key) = access_key_query_response.kind {
                access_key.nonce
            } else {
                warn!("Failed to get current nonce");
                continue;
            };

        let transaction = TransactionV0 {
            signer_id: near_signer.account_id.clone(),
            public_key: near_signer.public_key.clone(),
            nonce: current_nonce + 1,
            receiver_id: config.token_locker_id_testnet.clone(),
            block_hash: access_key_query_response.block_hash,
            actions: vec![near_primitives::transaction::Action::FunctionCall(
                Box::new(near_primitives::transaction::FunctionCallAction {
                    method_name: "sign_transfer".to_string(),
                    args: serde_json::json!({
                        "nonce": transfer_message.origin_nonce,
                        "fee_recepient": Some(config.token_locker_id_testnet.clone()),
                        "fee": Some(transfer_message.fee)
                    })
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

        match client.call(request).await {
            Ok(outcome) => {
                info!("Signed transfer: {:?}", outcome);
            }
            Err(err) => {
                error!("Failed to sign transfer: {}", err);
            }
        }
    }
}

pub async fn finalize_transfer(
    connector: Arc<Nep141Connector>,
    finalize_transfer_rx: &mut mpsc::UnboundedReceiver<Nep141LockerEvent>,
) {
    while let Some(log) = finalize_transfer_rx.recv().await {
        let Nep141LockerEvent::SignTransferEvent { .. } = &log else {
            error!("Expected SignTransferEvent, got: {:?}", log);
            continue;
        };

        match connector.finalize_deposit_omni_with_log(log).await {
            Ok(tx_hash) => {
                info!("Finalized deposit: {}", tx_hash);
            }
            Err(err) => {
                error!("Failed to finalize deposit: {}", err);
            }
        }
    }
}
