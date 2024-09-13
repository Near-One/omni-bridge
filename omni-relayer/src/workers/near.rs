use std::sync::Arc;

use futures::future::join_all;
use log::{error, info, warn};
use nep141_connector::Nep141Connector;

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

use crate::{defaults, utils};

pub async fn sign_transfer(
    config: crate::Config,
    redis_client: redis::Client,
    jsonrpc_client: JsonRpcClient,
    near_signer: InMemorySigner,
) {
    let redis_connection = redis_client
        .get_multiplexed_tokio_connection()
        .await
        .unwrap();

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events_test(
            &mut redis_connection_clone,
            "near_init_transfer_events".to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                defaults::SLEEP_TIME_AFTER_EVENTS_PROCESS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        while let Some((nonce, event)) = events.next_item().await {
            if let Ok(event) = serde_json::from_str::<Nep141LockerEvent>(&event) {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let jsonrpc_client = jsonrpc_client.clone();
                    let near_signer = near_signer.clone();

                    async move {
                        let Nep141LockerEvent::InitTransferEvent { transfer_message } = event
                        else {
                            warn!("Expected InitTransferEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received InitTransferEvent: {}", transfer_message.origin_nonce.0);

                        let Ok(access_key_query_response) = jsonrpc_client
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
                            return;
                        };

                        let current_nonce = if let QueryResponseKind::AccessKey(access_key) =
                            access_key_query_response.kind
                        {
                            access_key.nonce
                        } else {
                            warn!("Failed to get current nonce");
                            return;
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

                        match jsonrpc_client.call(request).await {
                            Ok(outcome) => {
                                info!("Signed transfer: {:?}", outcome);
                                utils::redis::remove_event_test(
                                    &mut redis_connection,
                                    "near_init_transfer_events",
                                    &nonce,
                                )
                                .await;
                            }
                            Err(err) => {
                                error!("Failed to sign transfer: {}", err);
                            }
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            defaults::SLEEP_TIME_AFTER_EVENTS_PROCESS,
        ))
        .await;
    }
}

pub async fn finalize_transfer(redis_client: redis::Client, connector: Arc<Nep141Connector>) {
    let redis_connection = redis_client
        .get_multiplexed_tokio_connection()
        .await
        .unwrap();

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(mut events) = utils::redis::get_events_test(
            &mut redis_connection_clone,
            "near_sign_transfer_events".to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                defaults::SLEEP_TIME_AFTER_EVENTS_PROCESS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        while let Some((nonce, event)) = events.next_item().await {
            if let Ok(event) = serde_json::from_str::<Nep141LockerEvent>(&event) {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let Nep141LockerEvent::SignTransferEvent { .. } = &event else {
                            error!("Expected SignTransferEvent, got: {:?}", event);
                            return;
                        };

                        match connector.finalize_deposit_omni_with_log(event).await {
                            Ok(tx_hash) => {
                                info!("Finalized deposit: {}", tx_hash);
                                utils::redis::remove_event_test(
                                    &mut redis_connection,
                                    "near_sign_transfer_events",
                                    &nonce,
                                )
                                .await;
                            }
                            Err(err) => {
                                error!("Failed to finalize deposit: {}", err);
                            }
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            defaults::SLEEP_TIME_AFTER_EVENTS_PROCESS,
        ))
        .await;
    }
}
