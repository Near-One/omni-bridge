use std::sync::Arc;

use alloy::rpc::types::{Log, TransactionReceipt};
use anyhow::Result;
use ethereum_types::H256;
use futures::future::join_all;
use log::{error, info, warn};

use near_primitives::types::AccountId;
use omni_connector::OmniConnector;
use omni_types::{
    locker_args::StorageDepositArgs, prover_result::ProofKind, ChainKind, OmniAddress,
};

use crate::{config, utils};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitTransferWithTimestamp {
    pub chain_kind: ChainKind,
    pub block_number: u64,
    pub log: Log,
    pub tx_logs: Option<TransactionReceipt>,
    pub creation_timestamp: i64,
    pub last_update_timestamp: Option<i64>,
}

pub async fn finalize_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
    near_signer: near_crypto::InMemorySigner,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection,
            utils::redis::ETH_WITHDRAW_EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        for (key, event) in events {
            if let Ok(init_transfer_with_timestamp) =
                serde_json::from_str::<InitTransferWithTimestamp>(&event)
            {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let connector = connector.clone();
                    let jsonrpc_client = jsonrpc_client.clone();
                    let mut redis_connection = redis_connection.clone();
                    let near_signer = near_signer.clone();

                    async move {
                        let current_timestamp = chrono::Utc::now().timestamp();

                        if current_timestamp
                            - init_transfer_with_timestamp
                                .last_update_timestamp
                                .unwrap_or_default()
                            < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
                        {
                            return;
                        }

                        let Ok(init_log) = init_transfer_with_timestamp
                            .log
                            .log_decode::<utils::evm::InitTransfer>()
                        else {
                            warn!(
                                "Failed to decode log as InitTransfer: {:?}",
                                init_transfer_with_timestamp.log
                            );
                            return;
                        };

                        info!("Received InitTransfer log");

                        let Some(tx_hash) = init_transfer_with_timestamp.log.transaction_hash
                        else {
                            warn!("No transaction hash in log: {:?}", init_log);
                            return;
                        };

                        let Ok(recipient) = init_log.inner.recipient.parse::<OmniAddress>() else {
                            warn!(
                                "Failed to parse recipient as OmniAddress: {:?}",
                                init_log.inner.recipient
                            );
                            return;
                        };
                        let Ok(token) = init_log.inner.token.parse::<AccountId>() else {
                            warn!(
                                "Failed to parse token as AccountId: {:?}",
                                init_log.inner.token
                            );
                            return;
                        };

                        if current_timestamp - init_transfer_with_timestamp.creation_timestamp
                            > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
                        {
                            warn!(
                                "Removing an old InitTransfer: {:?}",
                                init_transfer_with_timestamp
                            );
                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::ETH_WITHDRAW_EVENTS,
                                &key,
                            )
                            .await;
                            return;
                        }

                        // TODO: Use existing API to check if fee is sufficient here

                        let vaa = utils::evm::get_vaa(
                            connector.clone(),
                            init_transfer_with_timestamp.tx_logs,
                            &init_transfer_with_timestamp.log,
                            &config,
                        )
                        .await;

                        if vaa.is_none() {
                            let Ok(light_client_latest_block_number) =
                                utils::near::get_eth_light_client_last_block_number(
                                    &config,
                                    &jsonrpc_client,
                                )
                                .await
                            else {
                                warn!("Failed to get eth light client last block number");
                                return;
                            };

                            if init_transfer_with_timestamp.block_number
                                > light_client_latest_block_number
                            {
                                tokio::time::sleep(tokio::time::Duration::from_secs(
                                    utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                                ))
                                .await;
                                return;
                            }
                        }

                        let Some(topic) = init_transfer_with_timestamp.log.topic0() else {
                            warn!("No topic0 in log: {:?}", init_transfer_with_timestamp.log);
                            return;
                        };

                        let tx_hash = H256::from_slice(tx_hash.as_slice());

                        let Some(prover_args) = utils::evm::construct_prover_args(
                            &config,
                            vaa,
                            tx_hash,
                            H256::from_slice(topic.as_slice()),
                            ProofKind::InitTransfer,
                        )
                        .await
                        else {
                            return;
                        };

                        let storage_deposit_accounts =
                            if let OmniAddress::Near(near_recipient) = recipient {
                                let Ok(sender_has_storage_deposit) =
                                    utils::storage::has_storage_deposit(
                                        &jsonrpc_client,
                                        &token,
                                        &near_signer.account_id,
                                    )
                                    .await
                                else {
                                    warn!("Failed to check sender storage balance");
                                    return;
                                };
                                let Ok(recipient_has_storage_deposit) =
                                    utils::storage::has_storage_deposit(
                                        &jsonrpc_client,
                                        &token,
                                        &near_recipient,
                                    )
                                    .await
                                else {
                                    warn!("Failed to check recipient storage balance");
                                    return;
                                };

                                vec![
                                    (near_signer.account_id, !sender_has_storage_deposit),
                                    (near_recipient, !recipient_has_storage_deposit),
                                ]
                            } else {
                                Vec::new()
                            };

                        let storage_deposit_args = StorageDepositArgs {
                            token,
                            accounts: storage_deposit_accounts,
                        };
                        let fin_transfer_args = omni_connector::FinTransferArgs::NearFinTransfer {
                            chain_kind: init_transfer_with_timestamp.chain_kind,
                            storage_deposit_args,
                            prover_args,
                        };

                        match connector.fin_transfer(fin_transfer_args).await {
                            Ok(tx_hash) => {
                                info!("Finalized InitTransfer: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::ETH_WITHDRAW_EVENTS,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => error!("Failed to finalize InitTransfer: {}", err),
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}
